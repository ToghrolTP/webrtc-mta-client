use axum::{Json, http::StatusCode};
use rtc::peer_connection::sdp::RTCSessionDescription;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionDescription {
    pub sdp: String,
    pub r#type: String,
}

pub async fn offer_handler(
    Json(payload): Json<SessionDescription>,
) -> Result<Json<SessionDescription>, StatusCode> {
    if payload.r#type != "offer" {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Verify SDP parses as a valid session description
    RTCSessionDescription::offer(payload.sdp.clone()).map_err(|_| StatusCode::BAD_REQUEST)?;

    // Placeholder until WebRTC pipeline is wired in Task 3
    Err(StatusCode::NOT_IMPLEMENTED)
}
