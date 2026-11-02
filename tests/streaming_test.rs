use axum_test::{TestServer, WsMessage};
use std::fs;
use webrtc_mta_client::app;

#[tokio::test]
async fn test_live_streaming_broadcaster_to_listener() {
    let server = TestServer::builder()
        .http_transport()
        .build(app().await)
        .unwrap();

    // 1. Broadcaster connects to /ws/audio
    let mut broadcaster = server
        .get_websocket("/ws/audio")
        .await
        .into_websocket()
        .await;

    // 2. Broadcaster sends initial chunk before listener connects
    let chunk_before = b"chunk_before_listener_connected";
    broadcaster
        .send_message(WsMessage::Binary(chunk_before.to_vec().into()))
        .await;

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // 3. Listener connects to /ws/listen mid-stream
    let mut listener = server
        .get_websocket("/ws/listen")
        .await
        .into_websocket()
        .await;

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // 4. Broadcaster sends live audio chunk
    let live_chunk = b"live_chunk_data_456";
    broadcaster
        .send_message(WsMessage::Binary(live_chunk.to_vec().into()))
        .await;

    // 5. Listener should receive the live chunk in real-time
    let received_live = listener.receive_message().await;
    match received_live {
        WsMessage::Binary(bytes) => {
            assert_eq!(&bytes[..], live_chunk);
        }
        other => panic!("Expected binary live chunk, got: {:?}", other),
    }

    // 6. Broadcaster finishes
    broadcaster.send_message(WsMessage::Close(None)).await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Cleanup recordings created during test
    if let Ok(entries) = fs::read_dir("recordings") {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Ok(content) = fs::read(&path)
                && content.windows(15).any(|w| w == b"live_chunk_data")
            {
                let _ = fs::remove_file(path);
            }
        }
    }
}
