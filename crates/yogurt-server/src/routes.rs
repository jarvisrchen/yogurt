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
        // Granola-style meeting labels — same auth group as the Library
        // CRUD surface above.
        .merge(crate::api::labels::router())
        // Floating "Return to recording" pill (discoverability for
        // navigate-away-while-recording). Static segment — axum 0.8's
        // matchit router prefers literal segments over the `{id}` param
        // matcher in `api::meetings::router()` regardless of registration
        // order (same guarantee the `/api/meetings/search` route already
        // relies on), so `active` can never be swallowed as an id.
        // `route_order_guard` in tests/meetings_api.rs asserts this holds.
        .route("/api/meetings/active", get(active_recording))
        // MTG-11: the meeting-detection prompt. Literal segments beat
        // `{id}` in axum 0.8's matcher, same guarantee `active` relies on.
        .route("/api/meetings/detected", get(detected_meeting))
        .route(
            "/api/meetings/detected/dismiss",
            post(dismiss_detected_meeting),
        )
        // axum 0.8 path syntax: `{id}` (not `:id`).
        .route("/api/meetings/{id}/start", post(start_meeting))
        .route("/api/meetings/{id}/stop", post(stop_meeting))
        .route(
            "/api/meetings/{id}/audio-device",
            post(switch_meeting_audio_device),
        )
        .route("/api/meetings/{id}/mic-muted", post(set_meeting_mic_muted))
        // Phase 4 (Plan 04-03): hero augmented-notes endpoint.
        .route("/api/meetings/{id}/enhance", post(crate::enhance::enhance))
        // LLM-9: the note formats enhance can shape a summary into, for
        // the post-meeting picker.
        .route("/api/templates", get(crate::enhance::list_templates))
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

/// `GET /api/health` - unauthenticated liveness + identity probe.
///
/// `version` and `mode` are additive (CLI-4 / D5): `yogurt ctl` needs a way
/// to tell instances apart when a port scan finds more than one, and
/// `version` also answers "which binary is this" without a separate
/// `doctor` round trip. No `pid` - that would be new unauthenticated
/// information about the host with no consumer asking for it.
async fn health(State(state): State<AppState>) -> Json<Value> {
    Json(json!({
        "status": "ok",
        "service": "yogurt-server",
        "version": env!("CARGO_PKG_VERSION"),
        "mode": match state.mode {
            Mode::Dev => "dev",
            Mode::Release => "release",
        },
    }))
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
    // Deepgram key resolution: env var (dev override) → key file (the
    // Settings → Transcription field stores it there). Release builds have
    // no .env.local, so without the stored-key path cloud STT — the seeded
    // default — was unusable in the shipped binary.
    let mut stt_settings = crate::meetings::SttSettings::from(&g);
    if stt_settings.stt_provider == "cloud" && stt_settings.deepgram_api_key.is_none() {
        stt_settings.deepgram_api_key = state
            .keys
            .get(crate::meetings::DEEPGRAM_KEY_ID)
            .ok()
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
            // duration once the meeting ends — but only on a genuine first
            // start; see `start_stamp_patch`. Best-effort: the row exists
            // for meetings created via POST /api/meetings; older/direct
            // registry meetings simply skip the stamp.
            let repo = state.meeting_repo.clone();
            let id_str = id.to_string();
            // Feature: per-meeting STT engine provenance. Mirrors the model
            // resolution in `DeepgramStt::new` (crates/yogurt-stt/src/deepgram.rs)
            // — env override, default "nova-3" — so the stamped string always
            // matches what the cloud adapter actually connected with.
            let stt_engine = if g.stt_provider == "cloud" {
                let model =
                    std::env::var("YOGURT_DEEPGRAM_MODEL").unwrap_or_else(|_| "nova-3".into());
                format!("cloud \u{b7} {model}")
            } else {
                format!("local \u{b7} {}", g.stt_model)
            };
            let _ = tokio::task::spawn_blocking(move || -> anyhow::Result<yogurt_db::Meeting> {
                let existing = repo.get(&id_str)?;
                let mut patch = start_stamp_patch(existing.as_ref());
                patch.stt_engine = Some(stt_engine);
                repo.patch(&id_str, patch)
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
/// `GET /api/meetings/detected` — MTG-11. The meeting-looking window the
/// watcher last saw, or `null`.
///
/// Returns `null` while a recording is already running: "start
/// recording?" is noise once the answer is obviously yes.
async fn detected_meeting(State(state): State<AppState>) -> impl IntoResponse {
    let recording = state.meetings.active_recording().await.is_some();
    let st = state.detect.lock().await;
    match st.prompt(recording) {
        Some(m) => (StatusCode::OK, Json(json!(m))).into_response(),
        None => (StatusCode::OK, Json(Value::Null)).into_response(),
    }
}

/// `POST /api/meetings/detected/dismiss` — MTG-11. Suppress the prompt
/// for the call currently on screen. The next *different* meeting window
/// prompts again (see `DetectState::dismissed`).
async fn dismiss_detected_meeting(State(state): State<AppState>) -> impl IntoResponse {
    state.detect.lock().await.dismiss_current();
    (StatusCode::OK, Json(json!({ "status": "dismissed" })))
}

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
    // Truthful engine badge (not the Settings row, which may have been
    // flipped mid-recording and won't apply until the next start) — read
    // straight off the `Meeting` that `Registry::start` stamped. AUD-6:
    // `mic_muted` rides along the same lookup so a page reload or second
    // tab reflects the true mute state within this existing poll, with no
    // new WS plumbing.
    let (stt, mic_muted) = match state.meetings.get(&id).await {
        Some(m) => (*m.stt_engine.lock().await, *m.mic_muted.lock().await),
        None => (None, false),
    };
    let mut body = json!({
        "id": id.to_string(),
        "title": title,
        "started_at": started_at,
        "mic_muted": mic_muted,
    });
    if let Some(engine) = stt {
        body["stt"] = json!(engine);
    }
    (StatusCode::OK, Json(body)).into_response()
}

/// Compute the `MeetingPatch` fields `start_meeting` applies to the SQLite
/// row after a successful `Registry::start()` call.
///
/// `existing` is the row's state read immediately before this call (`None`
/// only if the row vanished between `Registry::start()` succeeding and this
/// read — best-effort, falls back to treating it as a first start).
///
/// - **Genuine first start** (no prior `ended_at`): stamp `started_at =
///   now`. This is the only authoritative source of the real recording
///   start time — the row's placeholder `started_at` from `POST
///   /api/meetings` is just the row's creation time.
/// - **Restart** (the meeting has an `ended_at` from a prior stop): leave
///   `started_at` untouched — overwriting it here IS the data-loss bug
///   this function exists to prevent, since a corrupted `started_at` also
///   fed the continuation-session clock (`meetings::session_offset_ms`) —
///   and clear the stale `ended_at` (`Some(None)`, the tri-state patch's
///   "explicitly clear" form) so the next `/stop` call stamps a fresh one.
///
/// The schema's own "unstarted" sentinel is `started_at = 0` (see
/// `V003__meetings.sql`), checked here as a belt-and-suspenders condition:
/// test fixtures (`test_support::seed_meeting`) seed rows exactly this way,
/// but `MeetingRepo::create` itself currently falls back to "now" rather
/// than the schema default, so `0` alone is not a reliable signal in real
/// usage — a set `ended_at` is what actually distinguishes "restarting" a
/// real meeting.
pub fn start_stamp_patch(existing: Option<&yogurt_db::Meeting>) -> yogurt_db::MeetingPatch {
    let is_restart = existing.is_some_and(|m| m.started_at != 0 && m.ended_at.is_some());
    if is_restart {
        yogurt_db::MeetingPatch {
            ended_at: Some(None),
            ..Default::default()
        }
    } else {
        yogurt_db::MeetingPatch {
            started_at: Some(now_unix_ms()),
            ..Default::default()
        }
    }
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

/// `POST /api/meetings/:id/mic-muted` — pause or resume the mic on an
/// actively-recording meeting (AUD-6). `Channel::System` is untouched.
#[derive(Deserialize)]
struct SetMicMutedRequest {
    muted: bool,
}

async fn set_meeting_mic_muted(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<SetMicMutedRequest>,
) -> impl IntoResponse {
    use crate::meetings::SwitchDeviceError;
    match state.meetings.set_mic_muted(&id, body.muted).await {
        Ok(()) => (
            StatusCode::OK,
            Json(json!({ "status": "ok", "muted": body.muted })),
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
