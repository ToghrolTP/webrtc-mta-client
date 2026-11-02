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

    // 2. Broadcaster sends initial header chunk
    let header_bytes = b"OggS\x00\x02\x00\x00\x00\x00\x00\x00\x00\x00init_header";
    broadcaster
        .send_message(WsMessage::Binary(header_bytes.to_vec().into()))
        .await;

    // Small delay to allow server to cache header
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // 3. Listener connects to /ws/listen
    let mut listener = server
        .get_websocket("/ws/listen")
        .await
        .into_websocket()
        .await;

    // 4. Listener should immediately receive the cached initial header
    let received_header = listener.receive_message().await;
    match received_header {
        WsMessage::Binary(bytes) => {
            assert_eq!(&bytes[..], header_bytes);
        }
        other => panic!("Expected binary header message, got: {:?}", other),
    }

    // 5. Broadcaster sends a live audio chunk
    let live_chunk = b"live_chunk_data_456";
    broadcaster
        .send_message(WsMessage::Binary(live_chunk.to_vec().into()))
        .await;

    // 6. Listener should receive the live chunk in real-time
    let received_live = listener.receive_message().await;
    match received_live {
        WsMessage::Binary(bytes) => {
            assert_eq!(&bytes[..], live_chunk);
        }
        other => panic!("Expected binary live chunk, got: {:?}", other),
    }

    // 7. Broadcaster finishes
    broadcaster.send_message(WsMessage::Close(None)).await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Cleanup recordings created during test
    if let Ok(entries) = fs::read_dir("recordings") {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Ok(content) = fs::read(&path)
                && content.windows(11).any(|w| w == b"init_header")
            {
                let _ = fs::remove_file(path);
            }
        }
    }
}
