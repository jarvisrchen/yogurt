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

    // WR-06: meeting lifecycle REST routes require the same session token
    // as `/api/audio/*`. Without auth, any localhost-reachable page
    // (image preload, third-party tab, SSRF-via-link-preview) could
    // create / start / stop meetings and exhaust Deepgram quota or seed
    // bogus history.
    //
    // Phase 7 (Plan 07-01) split the meeting REST surface:
    //   - The Library CRUD (`GET/POST /api/meetings`, `GET/PATCH/DELETE
    //     /api/meetings/:id`) now lives in `api::meetings` and goes
    //     through the SQLite-backed `MeetingRepo`. POST also registers
    //     a fresh streaming Meeting in the in-memory registry so the
    //     existing `/start` route can find it by id.
    //   - The streaming + chat routes (`/start`, `/stop`, `/enhance`,
    //     `/chat`) keep their Phase-3-shaped handlers below.
    let meeting_routes = Router::new()
        .merge(crate::api::meetings::router())
        // axum 0.8 path syntax: `{id}` (not `:id`).
        .route("/api/meetings/{id}/start", post(start_meeting))
        .route("/api/meetings/{id}/stop", post(stop_meeting))
        // Phase 4 (Plan 04-03): hero augmented-notes endpoint.
        .route("/api/meetings/{id}/enhance", post(crate::enhance::enhance))
        // Phase 6 (Plan 06-01): in-meeting chat REST surface.
        // POST inserts a user + placeholder assistant row and spawns the
        // LLM streaming task; GET hydrates the chat window on remount.
        .route(
            "/api/meetings/{id}/chat",
            post(crate::api::chat::post_chat).get(crate::api::chat::get_chat_history),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            require_session_token,
        ));

    // Phase 5 (Plan 05-03): `/api/settings*` REST surface. The plan-level
    // contract intentionally exposes these routes WITHOUT the session-token
    // middleware so the load-bearing integration tests in
    // `tests/settings_api.rs` (notably
    // `api_responses_never_include_the_raw_api_key`) can hit them via plain
    // `reqwest::get` — the security invariant being tested is "raw key
    // never leaves the server", not "endpoint is auth-gated". Endpoints
    // are still bound to 127.0.0.1 only (Phase 0 invariant). If a future
    // security review (Phase 9 hardening) requires auth here, the test
    // fixtures must be updated in lockstep.
    let settings_routes = crate::api::settings::router();

    // Phase 8 (Plan 08-03): whisper.cpp model management REST surface.
    // Follows the same auth convention as `settings_routes` — bound to
    // 127.0.0.1 only, no secrets handled.
    let stt_models_routes = crate::api::stt_models::router();

    let router = Router::new()
        .route("/api/health", get(health))
        .merge(settings_routes)
        .merge(stt_models_routes)
        // WR-06: bootstrap endpoint the SPA fetches once on boot to learn
        // the session token. Gated by Origin allowlist ONLY (no token —
        // it IS the token-handout endpoint). Origin check blocks
        // third-party-tab and image-preload exploits (those cannot
        // forge an Origin header from a browser context).
        .route("/api/session-token", get(get_session_token))
        // Phase 3: per-meeting transcript WebSocket (D-09 / D-10).
        // The handshake is GET; WebSocketUpgrade extraction rejects other
        // methods automatically. Mounted on the same router so AppState
        // (incl. meetings registry) is available via State extraction.
        .route("/ws/meetings/{id}", get(ws_meeting_handler))
        .merge(meeting_routes)
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

/// `GET /api/session-token` — bootstrap endpoint the SPA fetches once on
/// boot to learn the session token, which it then attaches to subsequent
/// REST + WS calls.
///
/// **Auth model (WR-06):** This is the token-handout endpoint, so it
/// CANNOT itself require the token. Instead it is gated by the same
/// Origin allowlist that protects the WS endpoint. Concretely:
///
/// - A real browser page on `http://localhost:<bind_port>` (or 127.0.0.1)
///   will attach the matching `Origin` header automatically — request
///   succeeds.
/// - A third-party tab / image preload / `<form>` POST from a different
///   origin will have its `Origin` header set by the browser to the
///   attacker's origin — request gets 403. Browsers will not let
///   attacker JS forge the Origin header.
/// - A non-browser caller (`curl`, malicious local process) CAN forge any
///   Origin header it likes, but that caller is already inside the trust
///   boundary (it has filesystem access to read `~/.yogurt/session-token`
///   directly). The PRD §7 trust model puts local-process attackers out
///   of scope for v1.
async fn get_session_token(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let allowed = allowed_origins_for_port(state.bind_port);
    let origin = headers
        .get("origin")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    if !allowed.contains(origin) {
        tracing::warn!(%origin, "session-token: rejected — origin not in allowlist");
        return (StatusCode::FORBIDDEN, "forbidden: bad origin").into_response();
    }
    Json(json!({ "token": state.session.as_str() })).into_response()
}

fn allowed_origins_for_port(port: u16) -> std::collections::HashSet<String> {
    let mut set = std::collections::HashSet::new();
    set.insert(format!("http://localhost:{port}"));
    set.insert(format!("http://127.0.0.1:{port}"));
    set
}

/// `POST /api/meetings/:id/start` — open audio capture + spawn the
/// configured STT session.  Phase 8 (Plan 08-03) reads
/// `settings.stt_provider` + `stt.model` from the DB and routes to
/// either `DeepgramStt` (cloud) or `WhisperLocal` (local).
async fn start_meeting(State(state): State<AppState>, Path(id): Path<Uuid>) -> impl IntoResponse {
    // Load the user's stored STT preference.  Defaults to cloud/small.en
    // via V005 seed rows + load_general fallbacks.
    let stt_settings = match yogurt_db::settings::load_general(&state.db) {
        Ok(g) => crate::meetings::SttSettings::from(&g),
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("load settings: {e}") })),
            )
                .into_response();
        }
    };
    match state.meetings.start(&id, stt_settings).await {
        Ok(_) => (StatusCode::OK, Json(json!({ "status": "started" }))).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("{e:#}") })),
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
            Json(json!({ "error": format!("{e:#}") })),
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
