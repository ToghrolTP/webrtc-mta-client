use std::sync::Arc;
use std::time::Duration;

use axum::http::StatusCode;
use axum_test::TestServer;
use rtc::peer_connection::configuration::RTCConfigurationBuilder;
use rtc::peer_connection::configuration::media_engine::{MIME_TYPE_OPUS, MediaEngine};
use rtc::peer_connection::sdp::RTCSessionDescription;
use rtc::rtp_transceiver::rtp_sender::{RTCRtpCodec, RTCRtpCodecParameters, RtpCodecKind};
use serde_json::json;
use tokio::time::timeout;
use webrtc::peer_connection::{PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler};
use webrtc::runtime::TokioRuntime;
use webrtc_mta_client::app;

#[tokio::test]
async fn test_full_system_smoke() {
    let server = TestServer::new(app().await).unwrap();

    // 1. GET /api/health
    let health = server.get("/api/health").await;
    assert_eq!(health.status_code(), StatusCode::OK);
    let health_json: serde_json::Value = health.json();
    assert_eq!(health_json["status"], "ok");

    // 2. GET / (index.html via fallback/root)
    let index = server.get("/").await;
    assert_eq!(index.status_code(), StatusCode::OK);
    assert!(index.text().contains("WebRTC Audio Recorder"));

    // 3. GET /static/app.js
    let script = server.get("/static/app.js").await;
    assert_eq!(script.status_code(), StatusCode::OK);
    assert!(script.text().contains("RTCPeerConnection"));

    // 4. GET /static/style.css
    let style = server.get("/static/style.css").await;
    assert_eq!(style.status_code(), StatusCode::OK);
    assert!(style.text().contains("--bg-color"));
}

struct DummyHandler;

#[async_trait::async_trait]
impl PeerConnectionEventHandler for DummyHandler {}

#[tokio::test]
async fn test_offer_answer_exchange_end_to_end() {
    let server = TestServer::new(app().await).unwrap();

    // Create a real WebRTC client peer connection to generate a valid SDP offer
    let mut media_engine = MediaEngine::default();
    let audio_codec = RTCRtpCodecParameters {
        rtp_codec: RTCRtpCodec {
            mime_type: MIME_TYPE_OPUS.to_owned(),
            clock_rate: 48000,
            channels: 2,
            sdp_fmtp_line: "".to_owned(),
            rtcp_feedback: vec![],
        },
        payload_type: 111,
    };
    media_engine
        .register_codec(audio_codec, RtpCodecKind::Audio)
        .expect("Failed to register opus codec");

    let config = RTCConfigurationBuilder::new().build();
    let runtime = Arc::new(TokioRuntime);
    let pc = PeerConnectionBuilder::new()
        .with_configuration(config)
        .with_media_engine(media_engine)
        .with_handler(Arc::new(DummyHandler) as Arc<dyn PeerConnectionEventHandler>)
        .with_runtime(runtime)
        .with_udp_addrs(vec!["0.0.0.0:0".to_string()])
        .build()
        .await
        .expect("Failed to build peer connection");

    pc.add_transceiver_from_kind(RtpCodecKind::Audio, None)
        .await
        .expect("Failed to add audio transceiver");

    let offer = pc.create_offer(None).await.expect("Failed to create offer");
    pc.set_local_description(offer.clone())
        .await
        .expect("Failed to set local description");

    // Send the offer to the server endpoint
    let response = server
        .post("/api/offer")
        .json(&json!({
            "type": "offer",
            "sdp": offer.sdp,
        }))
        .await;

    assert_eq!(response.status_code(), StatusCode::OK);

    let answer_payload: serde_json::Value = response.json();
    assert_eq!(answer_payload["type"], "answer");

    let answer_sdp = answer_payload["sdp"]
        .as_str()
        .expect("Answer SDP should be a string");
    assert!(!answer_sdp.is_empty(), "Answer SDP should not be empty");

    // Verify the returned SDP can be parsed as a valid answer and applied back to the peer connection
    let answer_desc = RTCSessionDescription::answer(answer_sdp.to_string())
        .expect("Answer SDP must be valid session description");
    pc.set_remote_description(answer_desc)
        .await
        .expect("Should set remote description successfully");

    // Close the peer connection
    let _ = timeout(Duration::from_millis(500), pc.close()).await;

    // Clean up any test recordings created in recordings/
    if let Ok(entries) = std::fs::read_dir("recordings") {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("ogg") {
                let _ = std::fs::remove_file(path);
            }
        }
        let _ = std::fs::remove_dir("recordings");
    }
}
