use axum::{
    Json, Router,
    extract::Request,
    http::{HeaderValue, header::CACHE_CONTROL},
    middleware::{self, Next},
    response::Response,
    routing::{get, post},
};
use serde::Serialize;
use tower_http::services::{ServeDir, ServeFile};

pub mod recorder;
pub mod signaling;
pub mod ws;

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
}

pub async fn health_handler() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

async fn no_cache_middleware(req: Request, next: Next) -> Response {
    let mut resp = next.run(req).await;
    resp.headers_mut().insert(
        CACHE_CONTROL,
        HeaderValue::from_static("no-cache, no-store, must-revalidate"),
    );
    resp
}

pub async fn app() -> Router {
    Router::new()
        .route("/api/health", get(health_handler))
        .route("/api/offer", post(signaling::offer_handler))
        .route("/ws/audio", get(ws::ws_handler))
        .route("/ws/listen", get(ws::ws_listen_handler))
        .route_service("/listen", ServeFile::new("static/listen.html"))
        .route_service("/listen.html", ServeFile::new("static/listen.html"))
        .nest_service("/static", ServeDir::new("static"))
        .fallback_service(ServeFile::new("static/index.html"))
        .layer(middleware::from_fn(no_cache_middleware))
}
