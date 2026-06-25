use axum::{
    routing::{any, get},
    Json, Router,
};
use serde_json::{json, Value};

use crate::assets::serve_embedded;
use crate::{AppState, Mode};

pub fn router(state: AppState) -> Router {
    let mode = state.mode;
    let router = Router::new()
        .route("/api/health", get(health))
        // MD-07: use `any()` so OPTIONS / POST / etc. against /ws ALSO go
        // through the Origin+token check rather than falling through to the
        // SPA handler (which would happily return index.html on OPTIONS /ws).
        // The handshake itself still requires GET; non-GET methods will be
        // rejected by ws_handler when WebSocketUpgrade extraction fails.
        .route("/ws", any(crate::ws::ws_handler))
        .with_state(state);

    match mode {
        Mode::Release => router.fallback(serve_embedded),
        Mode::Dev => router.fallback(crate::dev_proxy::proxy_to_vite),
    }
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok", "service": "yogurt-server" }))
}
