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
        // Quick task 260628-g71 DD-05: dedicated POST to fire the macOS
        // microphone permission dialog from the Welcome "Grant Microphone"
        // button. Idempotent / fire-and-forget; returns the combined
        // permission snapshot.
        .route(
            "/api/audio/microphone/request",
            post(audio::request_microphone),
        )
        // Quick task 260701-vjb: dedicated POST to fire the macOS Screen
        // Recording TCC prompt from the Welcome "Grant Screen Recording"
        // button. Fire-and-forget; returns the combined permission snapshot.
        .route(
            "/api/audio/screen-recording/request",
            post(audio::request_screen_recording),
        )
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
        // Floating "Return to recording" pill (discoverability for
        // navigate-away-while-recording). Static segment — axum 0.8's
        // matchit router prefers literal segments over the `{id}` param
        // matcher in `api::meetings::router()` regardless of registration
        // order (same guarantee the `/api/meetings/search` route already
        // relies on), so `active` can never be swallowed as an id.
        // `route_order_guard` in tests/meetings_api.rs asserts this holds.
        .route("/api/meetings/active", get(active_recording))
        // axum 0.8 path syntax: `{id}` (not `:id`).
        .route("/api/meetings/{id}/start", post(start_meeting))
        .route("/api/meetings/{id}/stop", post(stop_meeting))
        .route(
            "/api/meetings/{id}/audio-device",
            post(switch_meeting_audio_device),
        )
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

    // Phase 5 (Plan 05-03): `/api/settings*` REST surface.
    //
    // Hardening (2026-08-13, E2E security finding): these routes now require
    // the same session token as `/api/audio/*` and the meeting surface.
    // Previously they were left unauthed so `tests/settings_api.rs` could hit
    // them via plain `reqwest::get`; that left an exploitable gap — any
    // localhost-reaching page could POST `/api/settings/providers` to point
    // the active LLM provider at an attacker `base_url` (confirmed via an
    // unauthenticated cross-origin curl). The frontend already sends the
    // token on every settings call (`api/settings.ts` → `bearerFetch`), so
    // only the integration-test fixtures needed updating (in lockstep). The
    // `api_responses_never_include_the_raw_api_key` invariant is unchanged —
    // that test now authenticates but still asserts the raw key never leaves
    // the server.
    let settings_routes = crate::api::settings::router().layer(middleware::from_fn_with_state(
        state.clone(),
        require_session_token,
    ));

    // Phase 8 (Plan 08-03): whisper.cpp model management REST surface.
    // Same auth as `settings_routes` — the frontend (`api/stt.ts`) already
    // attaches the token via `bearerFetch`.
    let stt_models_routes = crate::api::stt_models::router().layer(middleware::from_fn_with_state(
        state.clone(),
        require_session_token,
    ));

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
        // Unknown /api/* paths must be an honest JSON 404, never fall
        // through to the SPA handler (in dev that meant a confusing
        // "cannot reach vite" 502 for a typo'd endpoint; in release it
        // returned index.html to an API client).
        .route("/api/{*rest}", any(api_not_found))
        .with_state(state);

    match mode {
        Mode::Release => router.fallback(serve_embedded),
        Mode::Dev => router.fallback(crate::dev_proxy::proxy_to_vite),
    }
}

async fn api_not_found(Path(rest): Path<String>) -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        Json(json!({ "error": format!("no such API endpoint: /api/{rest}") })),
    )
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
    // Browsers omit the `Origin` header on same-origin GET requests (per
    // Fetch spec) — without a fallback gate the SPA bootstrap 403s and the
    // app never loads. `Sec-Fetch-Site: same-origin` is sent by every
    // modern browser (Chrome 76+, Firefox 90+, Safari 16+) on every fetch
    // and cannot be forged by attacker JS, so it preserves the CSRF intent
    // for the empty-Origin case.
    let sec_fetch_site = headers
        .get("sec-fetch-site")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    let origin_in_allowlist = allowed.contains(origin);
    let same_origin_browser_fetch = origin.is_empty() && sec_fetch_site == "same-origin";

    if !origin_in_allowlist && !same_origin_browser_fetch {
        tracing::warn!(%origin, sec_fetch_site, "session-token: rejected — origin not in allowlist");
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
    // Load the user's stored settings.  Defaults to cloud/small.en via
    // V005 seed rows + load_general fallbacks; also carries the persisted
    // `audio_input_device` so a new recording opens the user's chosen mic
    // instead of always the OS default.
    let g = match yogurt_db::settings::load_general(&state.db) {
        Ok(g) => g,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("load settings: {e}") })),
            )
                .into_response();
        }
    };
    let mic_device = if g.audio_input_device.is_empty() {
        None
    } else {
        Some(g.audio_input_device.clone())
    };
    // Deepgram key resolution: env var (dev override) → Keychain (the
    // Settings → Transcription field stores it there). Release builds have
    // no .env.local, so without the Keychain path cloud STT — the seeded
    // default — was unusable in the shipped binary.
    let mut stt_settings = crate::meetings::SttSettings::from(&g);
    if stt_settings.stt_provider == "cloud" && stt_settings.deepgram_api_key.is_none() {
        let keys = state.keys.clone();
        // 10s bound: a wedged Keychain degrades to the actionable
        // "no Deepgram API key configured" error instead of hanging /start.
        stt_settings.deepgram_api_key = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            tokio::task::spawn_blocking(move || keys.get(crate::meetings::DEEPGRAM_KEY_ID)),
        )
        .await
        .ok()
        .and_then(|j| j.ok())
        .and_then(|r| r.ok())
        .flatten();
    }
    match state
        .meetings
        .start(
            &id,
            stt_settings,
            mic_device,
            Some(state.meeting_repo.clone()),
        )
        .await
    {
        Ok(_) => {
            // Stamp the actual recording start so the library shows a real
            // duration once the meeting ends. Best-effort: the row exists
            // for meetings created via POST /api/meetings; older/direct
            // registry meetings simply skip the stamp.
            let repo = state.meeting_repo.clone();
            let id_str = id.to_string();
            let _ = tokio::task::spawn_blocking(move || {
                repo.patch(
                    &id_str,
                    yogurt_db::MeetingPatch {
                        started_at: Some(now_unix_ms()),
                        ..Default::default()
                    },
                )
            })
            .await;
            (StatusCode::OK, Json(json!({ "status": "started" }))).into_response()
        }
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("{e:#}") })),
        )
            .into_response(),
    }
}

/// `GET /api/meetings/active` — the single currently-recording meeting, if
/// any. Backs the floating "Return to recording" pill (PRD discoverability
/// requirement: recording continues server-side across navigation, so the
/// UI needs a global way back). Polled every 5s by the frontend, so this
/// always returns `200` — `null` when nothing is recording, never `404`
/// (a 404 would just be constant background noise in the network log).
async fn active_recording(State(state): State<AppState>) -> impl IntoResponse {
    let Some(id) = state.meetings.active_recording().await else {
        return (StatusCode::OK, Json(Value::Null)).into_response();
    };
    // Best-effort title/started_at lookup — the SQLite row always exists in
    // practice (POST /api/meetings creates it before the registry entry can
    // start recording), but if it's ever missing, fall back to a generic
    // title and "just started" so the pill still renders something sane
    // rather than erroring the poll.
    let repo = state.meeting_repo.clone();
    let id_str = id.to_string();
    let row = tokio::task::spawn_blocking(move || repo.get(&id_str))
        .await
        .ok()
        .and_then(|r| r.ok())
        .flatten();
    let (title, started_at) = match row {
        Some(m) => (m.title, m.started_at),
        None => ("Recording".to_string(), now_unix_ms()),
    };
    (
        StatusCode::OK,
        Json(json!({ "id": id.to_string(), "title": title, "started_at": started_at })),
    )
        .into_response()
}

fn now_unix_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// `POST /api/meetings/:id/audio-device` — hot-swap the mic device on an
/// actively-recording meeting. Returns the resolved device name so the
/// toolbar picker can reflect the actual active device.
#[derive(Deserialize)]
struct SwitchDeviceRequest {
    device_id: String,
}

async fn switch_meeting_audio_device(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<SwitchDeviceRequest>,
) -> impl IntoResponse {
    use crate::meetings::SwitchDeviceError;
    match state.meetings.switch_mic_device(&id, body.device_id).await {
        Ok(device) => (
            StatusCode::OK,
            Json(json!({ "status": "switched", "device": device })),
        )
            .into_response(),
        Err(SwitchDeviceError::NotFound) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "meeting not found" })),
        )
            .into_response(),
        Err(SwitchDeviceError::NotRecording) => (
            StatusCode::CONFLICT,
            Json(json!({ "error": "meeting is not currently recording" })),
        )
            .into_response(),
        Err(SwitchDeviceError::Device(msg)) => {
            (StatusCode::BAD_REQUEST, Json(json!({ "error": msg }))).into_response()
        }
    }
}

/// `POST /api/meetings/:id/stop` — abort the meeting's supervisor task
/// (idempotent). Dropping the supervisor signals the audio thread to drop
/// the AudioStream (RAII stops cpal + SCK).
async fn stop_meeting(State(state): State<AppState>, Path(id): Path<Uuid>) -> impl IntoResponse {
    match state.meetings.stop(&id).await {
        Ok(_) => {
            // Stamp ended_at (first stop wins — repeat stops are no-ops) so
            // the library can show a real duration instead of a dash.
            let repo = state.meeting_repo.clone();
            let id_str = id.to_string();
            let _ = tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
                if let Some(m) = repo.get(&id_str)? {
                    if m.ended_at.is_none() {
                        repo.patch(
                            &id_str,
                            yogurt_db::MeetingPatch {
                                ended_at: Some(Some(now_unix_ms())),
                                ..Default::default()
                            },
                        )?;
                    }
                }
                Ok(())
            })
            .await;
            (StatusCode::OK, Json(json!({ "status": "stopped" }))).into_response()
        }
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
