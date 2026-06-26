use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{any, get},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::assets::serve_embedded;
use crate::{audio, AppState, Mode};

pub fn router(state: AppState) -> Router {
    let mode = state.mode;

    // Audio capture surface for Phase 5 settings + Phase 7 onboarding.
    // WR-08: require the same session token the /ws endpoint validates
    // (Phase 0 ws_auth pattern). Without auth, any localhost-reaching page
    // can enumerate the user's audio hardware (device names are fingerprint
    // material) or probe TCC state. Localhost-bind alone is not sufficient
    // defense against image-preload / iframe / SSRF-via-link-preview
    // attacks that can hit GETs without CORS protection.
    let audio_routes = Router::new()
        .route("/api/audio/devices", get(audio::get_devices))
        .route("/api/audio/permission", get(audio::get_permission))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            require_session_token,
        ));

    let router = Router::new()
        .route("/api/health", get(health))
        .merge(audio_routes)
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

/// Query-string carrier for `?token=<token>` on `/api/audio/*`. Matches the
/// WS endpoint's convention so the UI has one mental model for "how do I
/// authenticate to yogurt-server" (see `ws.rs` for the WS handler's use).
#[derive(Deserialize)]
struct AuthQuery {
    token: Option<String>,
}

/// Middleware: require a valid session token on every protected request.
///
/// Token sources (checked in order):
///   1. `Authorization: Bearer <token>` header — preferred for REST clients.
///   2. `?token=<token>` query string — matches the WS endpoint convention.
///
/// A `403 Forbidden` is returned for missing or mismatched tokens. The
/// candidate value is never logged (BL-02 — see `ws.rs`).
async fn require_session_token(
    State(state): State<AppState>,
    Query(q): Query<AuthQuery>,
    headers: HeaderMap,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    let from_header = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.to_string());

    let candidate = from_header.or(q.token);

    let Some(token) = candidate else {
        tracing::warn!("api: rejected — no session token presented");
        return (StatusCode::FORBIDDEN, "forbidden: missing token").into_response();
    };

    if !state.session.validate(&token) {
        tracing::warn!("api: rejected — session token mismatch");
        return (StatusCode::FORBIDDEN, "forbidden: bad token").into_response();
    }

    next.run(request).await
}
