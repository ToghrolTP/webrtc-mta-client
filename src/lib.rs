use axum::{
    Json, Router,
    routing::{get, post},
};
use serde_json::{Value, json};

pub mod signaling;

pub async fn health_handler() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

pub async fn app() -> Router {
    Router::new()
        .route("/api/health", get(health_handler))
        .route("/api/offer", post(signaling::offer_handler))
}
