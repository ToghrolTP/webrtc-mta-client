use axum::{
    Json, Router,
    routing::{get, post},
};
use serde::Serialize;
use tower_http::services::{ServeDir, ServeFile};

pub mod recorder;
pub mod signaling;

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
}

pub async fn health_handler() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

pub async fn app() -> Router {
    Router::new()
        .route("/api/health", get(health_handler))
        .route("/api/offer", post(signaling::offer_handler))
        .nest_service("/static", ServeDir::new("static"))
        .fallback_service(ServeFile::new("static/index.html"))
}
