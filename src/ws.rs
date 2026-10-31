use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use chrono::Local;
use std::fs;
use std::path::PathBuf;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

pub async fn ws_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_socket)
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

    tracing::info!("WebSocket audio stream connected");

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
