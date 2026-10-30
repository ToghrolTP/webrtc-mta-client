use axum::{Json, http::StatusCode};
use rtc::peer_connection::sdp::RTCSessionDescription;
use serde::{Deserialize, Serialize};

use crate::recorder::handle_webrtc_offer;

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

    // Validate offer SDP without unnecessary cloning
    let offer = RTCSessionDescription::offer(payload.sdp).map_err(|_| StatusCode::BAD_REQUEST)?;

    match handle_webrtc_offer(offer.sdp).await {
        Ok(answer_sdp) => Ok(Json(SessionDescription {
            sdp: answer_sdp,
            r#type: "answer".to_string(),
        })),
        Err(e) => {
            tracing::error!("Failed to process WebRTC offer: {:?}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
