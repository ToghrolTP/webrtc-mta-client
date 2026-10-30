use axum::{
    Json, Router,
    routing::{get, post},
};
use serde_json::{Value, json};
use tower_http::services::{ServeDir, ServeFile};

pub mod recorder;
pub mod signaling;

pub async fn health_handler() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

pub async fn app() -> Router {
    Router::new()
        .route("/api/health", get(health_handler))
        .route("/api/offer", post(signaling::offer_handler))
        .nest_service("/static", ServeDir::new("static"))
        .fallback_service(ServeFile::new("static/index.html"))
}
