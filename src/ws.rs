use axum::body::Bytes;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use chrono::Local;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock};
use tokio::fs::File;
use tokio::io::{AsyncSeekExt, AsyncWriteExt, SeekFrom};
use tokio::sync::broadcast;

struct StreamHub {
    sender: broadcast::Sender<Bytes>,
}

static STREAM_HUB: LazyLock<Arc<StreamHub>> = LazyLock::new(|| {
    let (tx, _rx) = broadcast::channel(256);
    Arc::new(StreamHub { sender: tx })
});

pub async fn ws_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_socket)
}

pub async fn ws_listen_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_listener_socket)
}

fn create_wav_header(
    data_len: u32,
    sample_rate: u32,
    channels: u16,
    bits_per_sample: u16,
) -> [u8; 44] {
    let mut header = [0u8; 44];
    let file_len = data_len.saturating_add(36);
    let byte_rate = sample_rate * (channels as u32) * (bits_per_sample as u32) / 8;
    let block_align = channels * bits_per_sample / 8;

    header[0..4].copy_from_slice(b"RIFF");
    header[4..8].copy_from_slice(&file_len.to_le_bytes());
    header[8..12].copy_from_slice(b"WAVE");
    header[12..16].copy_from_slice(b"fmt ");
    header[16..20].copy_from_slice(&16u32.to_le_bytes()); // PCM subchunk size
    header[20..22].copy_from_slice(&1u16.to_le_bytes()); // Audio format 1 = PCM
    header[22..24].copy_from_slice(&channels.to_le_bytes());
    header[24..28].copy_from_slice(&sample_rate.to_le_bytes());
    header[28..32].copy_from_slice(&byte_rate.to_le_bytes());
    header[32..34].copy_from_slice(&block_align.to_le_bytes());
    header[34..36].copy_from_slice(&bits_per_sample.to_le_bytes());
    header[36..40].copy_from_slice(b"data");
    header[40..44].copy_from_slice(&data_len.to_le_bytes());

    header
}

async fn handle_socket(mut socket: WebSocket) {
    let recordings_dir = PathBuf::from("recordings");
    if let Err(e) = fs::create_dir_all(&recordings_dir) {
        tracing::error!("Failed to create recordings dir: {:?}", e);
        return;
    }

    let ts = Local::now().format("%Y%m%d_%H%M%S_%3f").to_string();
    let mut file: Option<File> = None;
    let mut file_path = PathBuf::new();
    let mut total_pcm_bytes = 0u32;
    let mut is_wav = false;

    tracing::info!("Broadcaster connected to audio stream");

    while let Some(msg_result) = socket.recv().await {
        let msg = match msg_result {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("WebSocket recv error: {:?}", e);
                break;
            }
        };

        match msg {
            Message::Binary(data) => {
                if file.is_none() {
                    let (ext, is_pcm) = if data.starts_with(b"OggS") {
                        ("ogg", false)
                    } else if data.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
                        ("webm", false)
                    } else {
                        ("wav", true)
                    };
                    is_wav = is_pcm;

                    let path = recordings_dir.join(format!("recording_{}.{}", ts, ext));
                    match File::create(&path).await {
                        Ok(mut f) => {
                            if is_wav {
                                let dummy_hdr = create_wav_header(0, 24000, 1, 16);
                                if let Err(e) = f.write_all(&dummy_hdr).await {
                                    tracing::error!("Failed writing WAV header: {:?}", e);
                                    break;
                                }
                            }
                            file_path = path;
                            file = Some(f);
                            tracing::info!("Created recording file: {:?}", file_path);
                        }
                        Err(e) => {
                            tracing::error!("Failed creating file {:?}: {:?}", path, e);
                            break;
                        }
                    }
                }

                // Broadcast live chunk to all connected listeners
                let _ = STREAM_HUB.sender.send(data.clone());

                if let Some(ref mut f) = file {
                    total_pcm_bytes = total_pcm_bytes.saturating_add(data.len() as u32);
                    if let Err(e) = f.write_all(&data).await {
                        tracing::error!("Error writing audio chunk: {:?}", e);
                        break;
                    }
                }
            }
            Message::Close(_) => break,
            Message::Ping(p) => {
                let _ = socket.send(Message::Pong(p)).await;
            }
            _ => {}
        }
    }

    if let Some(mut f) = file {
        if is_wav {
            let file_size_minus_8 = total_pcm_bytes.saturating_add(36);
            let _ = f.seek(SeekFrom::Start(4)).await;
            let _ = f.write_all(&file_size_minus_8.to_le_bytes()).await;
            let _ = f.seek(SeekFrom::Start(40)).await;
            let _ = f.write_all(&total_pcm_bytes.to_le_bytes()).await;
        }
        let _ = f.flush().await;
        tracing::info!(
            "Recording saved: {:?} ({} bytes)",
            file_path,
            total_pcm_bytes
        );
        let ack = serde_json::json!({
            "status": "saved",
            "file": file_path.to_string_lossy(),
            "bytes": total_pcm_bytes
        });
        let _ = socket.send(Message::Text(ack.to_string().into())).await;
    }
}

async fn handle_listener_socket(mut socket: WebSocket) {
    tracing::info!("Listener connected to stream");
    let mut rx = STREAM_HUB.sender.subscribe();

    loop {
        tokio::select! {
            res = rx.recv() => {
                match res {
                    Ok(chunk) => {
                        if let Err(e) = socket.send(Message::Binary(chunk)).await {
                            tracing::info!("Listener connection dropped: {:?}", e);
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::warn!("Listener lagged by {} messages", skipped);
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        tracing::info!("Broadcast channel closed");
                        break;
                    }
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Ping(p))) => {
                        let _ = socket.send(Message::Pong(p)).await;
                    }
                    _ => {}
                }
            }
        }
    }
    tracing::info!("Listener stream ended");
}
