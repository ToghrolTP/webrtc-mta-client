use axum::http::StatusCode;
use axum_test::TestServer;
use serde_json::json;
use webrtc_mta_client::app;

#[tokio::test]
async fn test_offer_rejects_empty_payload() {
    let server = TestServer::new(app().await).unwrap();
    let response = server.post("/api/offer").json(&json!({})).await;
    assert_eq!(response.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn test_offer_rejects_malformed_sdp() {
    let server = TestServer::new(app().await).unwrap();
    let response = server
        .post("/api/offer")
        .json(&json!({
            "sdp": "invalid-sdp-content",
            "type": "offer"
        }))
        .await;
    assert_eq!(response.status_code(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_offer_rejects_non_offer_type() {
    let server = TestServer::new(app().await).unwrap();
    let response = server
        .post("/api/offer")
        .json(&json!({
            "sdp": "v=0\r\no=- 0 0 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\n",
            "type": "answer"
        }))
        .await;
    assert_eq!(response.status_code(), StatusCode::BAD_REQUEST);
}
