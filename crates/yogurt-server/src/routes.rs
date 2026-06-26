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

    // WR-06: meeting lifecycle REST routes now require the same session
    // token as `/api/audio/*`. Without auth, any localhost-reachable page
    // (image preload, third-party tab, SSRF-via-link-preview) could
    // create / start / stop meetings and exhaust Deepgram quota or seed
    // bogus history.
    let meeting_routes = Router::new()
        .route("/api/meetings", post(create_meeting))
        // axum 0.8 path syntax: `{id}` (not `:id`). The plan's superpowers
        // source was written against 0.7 — the user prompt acceptance
        // criteria check for the conceptual route shape, which this matches.
        .route("/api/meetings/{id}/start", post(start_meeting))
        .route("/api/meetings/{id}/stop", post(stop_meeting))
        // Phase 4 (Plan 04-03): hero augmented-notes endpoint. Same auth
        // model as the other meeting routes (session token via
        // Authorization header OR `?token=` query string).
        .route("/api/meetings/{id}/enhance", post(crate::enhance::enhance))
        // Phase 4 (Plan 04-04): GET the persisted meeting row — used by the
        // MeetingPost route to hydrate on direct-link / refresh (NOTES-13
        // step 10 persistence gate). Returns enriched_md + notes_md +
        // transcript_json + title + started_at_unix_ms + ended_at_unix_ms.
        // 404 if the meeting hasn't been enhanced yet (row only exists
        // after first enhance per Plan 04-03's UPSERT contract).
        .route("/api/meetings/{id}", get(get_meeting))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            require_session_token,
        ));

    let router = Router::new()
        .route("/api/health", get(health))
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

/// `GET /api/meetings/:id` — read the persisted meeting row from SQLite.
///
/// Used by the Plan 04-04 `MeetingPost` route to hydrate on direct-link
/// or refresh — `location.state.enrichedMd` lives for one navigation,
/// after which the post page falls back to this endpoint. Returns:
///
/// ```json
/// {
///   "id": "<uuid>",
///   "title": "string|null",
///   "notes_md": "string|null",
///   "transcript_json": "string|null",
///   "enriched_md": "string|null",
///   "started_at_unix_ms": 1234567890,
///   "ended_at_unix_ms": null
/// }
/// ```
///
/// 404 if the meeting hasn't been enhanced yet (the row only exists after
/// the first `/enhance` per Plan 04-03's UPSERT contract — pre-enhance
/// meetings live only in `AppState.meetings`).
///
/// `enriched_doc_json` is intentionally NOT returned — the browser uses
/// `enriched_md` (the wire-format markdown) to reconstruct the editor via
/// `markdownToHtml` + `setContent`. The JSON column exists for future
/// surfaces (Phase 7 library scroll-restoration).
async fn get_meeting(State(state): State<AppState>, Path(id): Path<Uuid>) -> impl IntoResponse {
    let id_str = id.to_string();
    // HI-1: use the read pool for SELECT queries. The writer Mutex was
    // serializing every GET behind every concurrent enhance UPSERT — a
    // 200ms LLM-bound write would stall every page refresh on the post
    // route. The read pool is sized to 4 connections with `query_only=ON`
    // so SELECTs go in parallel.
    let reader = state.storage.read();
    let row = {
        let conn = match reader.lock() {
            Ok(c) => c,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": format!("db lock: {e}") })),
                )
                    .into_response();
            }
        };
        conn.query_row(
            r#"SELECT id, title, notes_md, transcript_json, enriched_md,
                       started_at, ended_at
                 FROM meetings WHERE id = ?1"#,
            rusqlite::params![id_str],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, Option<String>>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, Option<String>>(4)?,
                    r.get::<_, Option<i64>>(5)?,
                    r.get::<_, Option<i64>>(6)?,
                ))
            },
        )
    };
    match row {
        Ok((id, title, notes_md, transcript_json, enriched_md, started, ended)) => Json(json!({
            "id": id,
            "title": title,
            "notes_md": notes_md,
            "transcript_json": transcript_json,
            "enriched_md": enriched_md,
            "started_at_unix_ms": started,
            "ended_at_unix_ms": ended,
        }))
        .into_response(),
        Err(rusqlite::Error::QueryReturnedNoRows) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("meeting {id_str} not found") })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("db query: {e}") })),
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
