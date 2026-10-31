use axum_test::{TestServer, WsMessage};
use std::fs;
use webrtc_mta_client::app;

#[tokio::test]
async fn test_ws_audio_recording() {
    let server = TestServer::builder()
        .http_transport()
        .build(app().await)
        .unwrap();

    let mut ws = server
        .get_websocket("/ws/audio")
        .await
        .into_websocket()
        .await;

    let dummy_audio = b"OggS\x00\x02\x00\x00\x00\x00\x00\x00\x00\x00test_payload";
    ws.send_message(WsMessage::Binary(dummy_audio.to_vec().into()))
        .await;
    ws.send_message(WsMessage::Close(None)).await;

    // Give server a moment to finish writing
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let mut found = false;
    if let Ok(entries) = fs::read_dir("recordings") {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(ext) = path.extension().and_then(|s| s.to_str())
                && (ext == "ogg" || ext == "webm")
                && let Ok(content) = fs::read(&path)
                && content.windows(12).any(|w| w == b"test_payload")
            {
                found = true;
                let _ = fs::remove_file(path);
                break;
            }
        }
    }
    assert!(found, "Audio file should be saved and contain sent payload");
}
