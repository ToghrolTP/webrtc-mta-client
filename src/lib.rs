use axum::{Json, Router, routing::get};
use serde_json::{Value, json};

pub async fn health_handler() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

pub async fn app() -> Router {
    Router::new().route("/api/health", get(health_handler))
}
