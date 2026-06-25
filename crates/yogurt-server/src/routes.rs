use axum::{routing::get, Json, Router};
use serde_json::{json, Value};

use crate::assets::serve_embedded;
use crate::{AppState, Mode};

pub fn router(state: AppState) -> Router {
    let mode = state.mode;
    let router = Router::new()
        .route("/api/health", get(health))
        .with_state(state.clone());

    let router = match mode {
        Mode::Release => router.fallback(serve_embedded),
        Mode::Dev => router.fallback(crate::dev_proxy::proxy_to_vite),
    };

    router
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok", "service": "yogurt-server" }))
}
