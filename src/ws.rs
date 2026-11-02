use axum::body::Bytes;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use chrono::Local;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, RwLock};
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::sync::broadcast;

struct StreamHub {
    sender: broadcast::Sender<Bytes>,
    header: RwLock<Option<Bytes>>,
}

static STREAM_HUB: LazyLock<Arc<StreamHub>> = LazyLock::new(|| {
    let (tx, _rx) = broadcast::channel(256);
    Arc::new(StreamHub {
        sender: tx,
        header: RwLock::new(None),
    })
});

pub async fn ws_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_socket)
}

pub async fn ws_listen_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_listener_socket)
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
    let mut total_bytes = 0usize;

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
                    if let Ok(mut h) = STREAM_HUB.header.write() {
                        *h = Some(data.clone());
                    }

                    let ext = if data.starts_with(b"OggS") {
                        "ogg"
                    } else {
                        "webm"
                    };
                    let path = recordings_dir.join(format!("recording_{}.{}", ts, ext));
                    match File::create(&path).await {
                        Ok(f) => {
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

                let _ = STREAM_HUB.sender.send(data.clone());

                if let Some(ref mut f) = file {
                    total_bytes += data.len();
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

    if let Ok(mut h) = STREAM_HUB.header.write() {
        *h = None;
    }

    if let Some(mut f) = file {
        let _ = f.flush().await;
        tracing::info!("Recording saved: {:?} ({} bytes)", file_path, total_bytes);
        let ack = serde_json::json!({
            "status": "saved",
            "file": file_path.to_string_lossy(),
            "bytes": total_bytes
        });
        let _ = socket.send(Message::Text(ack.to_string().into())).await;
    }
}

async fn handle_listener_socket(mut socket: WebSocket) {
    tracing::info!("Listener connected to stream");

    let initial_header = STREAM_HUB.header.read().ok().and_then(|h| h.clone());
    if let Some(header) = initial_header
        && let Err(e) = socket.send(Message::Binary(header)).await
    {
        tracing::warn!("Failed sending initial stream header to listener: {:?}", e);
        return;
    }

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
