use axum::http::StatusCode;
use axum_test::TestServer;
use webrtc_mta_client::app;

#[tokio::test]
async fn test_serves_index_html() {
    let server = TestServer::new(app().await).unwrap();
    let response = server.get("/").await;
    assert_eq!(response.status_code(), StatusCode::OK);
    assert!(response.text().contains("WebRTC Audio Recorder"));
}

#[tokio::test]
async fn test_serves_static_assets() {
    let server = TestServer::new(app().await).unwrap();
    let response = server.get("/static/style.css").await;
    assert_eq!(response.status_code(), StatusCode::OK);
    assert!(response.text().contains("--bg-color"));

    let response = server.get("/static/app.js").await;
    assert_eq!(response.status_code(), StatusCode::OK);
    assert!(response.text().contains("RTCPeerConnection"));

    let response = server.get("/listen").await;
    assert_eq!(response.status_code(), StatusCode::OK);
    assert!(response.text().contains("Live Audio Player"));

    let response = server.get("/static/listen.js").await;
    assert_eq!(response.status_code(), StatusCode::OK);
    assert!(response.text().contains("AudioContext"));
    assert!(response.text().contains("MediaSource"));
}
