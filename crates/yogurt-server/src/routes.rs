use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{any, get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::assets::serve_embedded;
use crate::ws::ws_meeting_handler;
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
        // Phase 3: meeting lifecycle REST surface (D-12).
        .route("/api/meetings", post(create_meeting))
        // axum 0.8 path syntax: `{id}` (not `:id`). The plan's superpowers
        // source was written against 0.7 — the user prompt acceptance
        // criteria check for the conceptual route shape, which this matches.
        .route("/api/meetings/{id}/start", post(start_meeting))
        .route("/api/meetings/{id}/stop", post(stop_meeting))
        // Phase 3: per-meeting transcript WebSocket (D-09 / D-10).
        // The handshake is GET; WebSocketUpgrade extraction rejects other
        // methods automatically. Mounted on the same router so AppState
        // (incl. meetings registry) is available via State extraction.
        .route("/ws/meetings/{id}", get(ws_meeting_handler))
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

/// `POST /api/meetings` — create a new meeting and return its UUID v7 id.
///
/// Body: empty. Returns `{ "id": <uuid>, "created_at_ms": <u64> }` (D-12).
/// The meeting is in-memory only in Phase 3; Phase 7 will persist via SQLite.
async fn create_meeting(State(state): State<AppState>) -> Json<Value> {
    let m = state.meetings.create().await;
    Json(json!({ "id": m.id, "created_at_ms": m.created_at_ms }))
}

/// `POST /api/meetings/:id/start` — open audio capture + spawn the Deepgram
/// STT session (D-12). 200 `{"status":"started"}` on success; 400 with
/// `{"error":<reason>}` on any failure (notably missing
/// `YOGURT_DEEPGRAM_API_KEY` per D-07).
async fn start_meeting(State(state): State<AppState>, Path(id): Path<Uuid>) -> impl IntoResponse {
    match state.meetings.start(&id).await {
        Ok(_) => (StatusCode::OK, Json(json!({ "status": "started" }))).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// `POST /api/meetings/:id/stop` — abort the meeting's supervisor task
/// (idempotent). Dropping the supervisor signals the audio thread to drop
/// the AudioStream (RAII stops cpal + SCK).
async fn stop_meeting(State(state): State<AppState>, Path(id): Path<Uuid>) -> impl IntoResponse {
    match state.meetings.stop(&id).await {
        Ok(_) => (StatusCode::OK, Json(json!({ "status": "stopped" }))).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
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
