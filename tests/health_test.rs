use axum::http::StatusCode;
use axum_test::TestServer;
use webrtc_mta_client::app;

#[tokio::test]
async fn test_health_check() {
    let router = app().await;
    let server = TestServer::new(router).expect("Failed to create test server");

    let response = server.get("/api/health").await;

    assert_eq!(response.status_code(), StatusCode::OK);
    assert_eq!(response.json::<serde_json::Value>()["status"], "ok");
}
