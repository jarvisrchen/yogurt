use axum::{routing::get, Json, Router};
use serde_json::{json, Value};

use crate::assets::serve_embedded;
use crate::Mode;

pub fn router(mode: Mode) -> Router {
    let router = Router::new().route("/api/health", get(health));

    match mode {
        Mode::Release => router.fallback(serve_embedded),
        Mode::Dev => router.fallback(crate::dev_proxy::proxy_to_vite),
    }
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok", "service": "yogurt-server" }))
}
