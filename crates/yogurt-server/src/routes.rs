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

    // Session token required: device names are fingerprint material and TCC state is probeable.
    // A localhost bind alone does not stop image-preload / iframe / SSRF-via-link-preview GETs.
    let audio_routes = Router::new()
        .route("/api/audio/devices", get(audio::get_devices))
        .route("/api/audio/output-devices", get(audio::get_output_devices))
        .route("/api/audio/echo/test", post(audio::test_echo))
        .route("/api/audio/permission", get(audio::get_permission))
        .route(
            "/api/audio/microphone/request",
            post(audio::request_microphone),
        )
        .route(
            "/api/audio/screen-recording/request",
            post(audio::request_screen_recording),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            require_session_token,
        ));

    // Session token required: otherwise any localhost-reachable page could create, start,
    // or stop meetings, exhaust Deepgram quota, or seed bogus history.
    // Library CRUD lives in `api::meetings`. POST there also registers a streaming Meeting
    // in the in-memory registry so `/start` can find it by id.
    let meeting_routes = Router::new()
        .merge(crate::api::meetings::router())
        .merge(crate::api::labels::router())
        .merge(crate::api::cloud::router())
        .merge(crate::granola_import::router())
        // axum 0.8 prefers literal segments over `{id}` regardless of registration order,
        // so `active` and `detected` are never swallowed as ids.
        // `route_order_guard` in tests/meetings_api.rs asserts this.
        .route("/api/meetings/active", get(active_recording))
        .route("/api/meetings/detected", get(detected_meeting))
        .route(
            "/api/meetings/detected/dismiss",
            post(dismiss_detected_meeting),
        )
        .route("/api/meetings/{id}/start", post(start_meeting))
        .route("/api/meetings/{id}/stop", post(stop_meeting))
        .route("/api/meetings/{id}/keep-recording", post(keep_recording))
        .route(
            "/api/meetings/{id}/audio-device",
            post(switch_meeting_audio_device),
        )
        .route("/api/meetings/{id}/mic-muted", post(set_meeting_mic_muted))
        .route("/api/meetings/{id}/echo", post(set_meeting_echo))
        .route("/api/meetings/{id}/enhance", post(crate::enhance::enhance))
        .route("/api/templates", get(crate::enhance::list_templates))
        // POST inserts a user + placeholder assistant row and spawns the LLM streaming task.
        .route(
            "/api/meetings/{id}/chat",
            post(crate::api::chat::post_chat).get(crate::api::chat::get_chat_history),
        )
        .route(
            "/api/chat",
            post(crate::api::library_chat::post_library_chat),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            require_session_token,
        ));

    // Session token required: otherwise any localhost-reaching page could POST
    // `/api/settings/providers` and point the active LLM provider at an attacker `base_url`.
    let settings_routes = crate::api::settings::router().layer(middleware::from_fn_with_state(
        state.clone(),
        require_session_token,
    ));

    let stt_models_routes = crate::api::stt_models::router().layer(middleware::from_fn_with_state(
        state.clone(),
        require_session_token,
    ));

    let router = Router::new()
        .route("/api/health", get(health))
        .merge(settings_routes)
        .merge(stt_models_routes)
        // Gated by the Origin allowlist only, since it hands out the token.
        .route("/api/session-token", get(get_session_token))
        .route("/ws/meetings/{id}", get(ws_meeting_handler))
        .merge(meeting_routes)
        .merge(audio_routes)
        // `any()` so non-GET methods hit the Origin+token check instead of falling
        // through to the SPA handler, which would return index.html.
        .route("/ws", any(crate::ws::ws_handler))
        // Unknown /api/* paths get a JSON 404 rather than the SPA handler
        // (a confusing vite 502 in dev, index.html in release).
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

/// Unauthenticated, so it carries no `pid`. `version` and `mode` let `yogurt ctl`
/// tell instances apart when a port scan finds several.
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

/// Hands the SPA its session token, so it cannot require one. Gated by the Origin
/// allowlist instead: browsers set the attacker's Origin on cross-origin requests and
/// attacker JS cannot forge it. A non-browser caller can forge Origin but already has
/// filesystem access to the token file, and local-process attackers are out of scope.
async fn get_session_token(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let allowed = allowed_origins_for_port(state.bind_port);
    let origin = headers
        .get("origin")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    // Browsers omit `Origin` on same-origin GETs, so fall back to `Sec-Fetch-Site`,
    // which attacker JS cannot forge.
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

/// `POST /api/meetings/:id/start`: open audio capture and spawn the STT session
/// chosen by `settings.stt_provider`.
async fn start_meeting(State(state): State<AppState>, Path(id): Path<Uuid>) -> impl IntoResponse {
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
    // Env var (dev override), then the key file. Release builds have no .env.local.
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
            // Best-effort: registry meetings without a row skip the stamp.
            let repo = state.meeting_repo.clone();
            let id_str = id.to_string();
            // Mirrors the model resolution in `DeepgramStt::new` (env override, default "nova-3").
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
            // Best-effort. `audio_echo_enabled` can be a stale `true` from before the
            // feature was turned off, so also gate on the feature toggle.
            if g.audio_echo_feature && g.audio_echo_enabled {
                if let Err(e) = state
                    .meetings
                    .set_echo(
                        &id,
                        true,
                        g.audio_echo_output_device.clone(),
                        g.audio_echo_buffer,
                    )
                    .await
                {
                    tracing::warn!(
                        error = ?e,
                        meeting_id = %id,
                        "failed to start mic echo at recording start; continuing without it"
                    );
                }
            }
            (StatusCode::OK, Json(json!({ "status": "started" }))).into_response()
        }
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("{e:#}") })),
        )
            .into_response(),
    }
}

/// The meeting-looking window the watcher last saw, or `null`.
/// Also `null` while recording, when a "start recording?" prompt is noise.
async fn detected_meeting(State(state): State<AppState>) -> impl IntoResponse {
    let recording = state.meetings.active_recording().await.is_some();
    let st = state.detect.lock().await;
    match st.prompt(recording) {
        Some(m) => (StatusCode::OK, Json(json!(m))).into_response(),
        None => (StatusCode::OK, Json(Value::Null)).into_response(),
    }
}

/// Suppress the prompt for the call on screen. The next different meeting window prompts again.
async fn dismiss_detected_meeting(State(state): State<AppState>) -> impl IntoResponse {
    state.detect.lock().await.dismiss_current();
    (StatusCode::OK, Json(json!({ "status": "dismissed" })))
}

/// The currently-recording meeting, for the "Return to recording" pill.
/// Polled every 5s, so it returns `200` with `null` rather than a noisy `404`.
async fn active_recording(State(state): State<AppState>) -> impl IntoResponse {
    let Some(id) = state.meetings.active_recording().await else {
        return (StatusCode::OK, Json(Value::Null)).into_response();
    };
    // The row should always exist, but fall back rather than error the poll.
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
    // Read the engine off the `Meeting`, not Settings, which may have changed mid-recording.
    let (stt, mic_muted, echo_enabled, last_audible) = match state.meetings.get(&id).await {
        Some(m) => (
            *m.stt_engine.lock().await,
            *m.mic_muted.lock().await,
            *m.echo_enabled.lock().await,
            Some(m.last_audible_ms.load(std::sync::atomic::Ordering::Relaxed)),
        ),
        None => (None, false, false, None),
    };
    let auto_stop_at = last_audible
        .filter(|_| crate::detect::auto_stop_enabled(&state.db))
        .and_then(|last| {
            match crate::detect::SilencePolicy::DEFAULT.check(last, now_unix_ms() as u64) {
                crate::detect::Silence::Warning { stop_at_ms } => Some(stop_at_ms),
                _ => None,
            }
        });
    let mut body = json!({
        "id": id.to_string(),
        "title": title,
        "started_at": started_at,
        "mic_muted": mic_muted,
        "echo_enabled": echo_enabled,
        "auto_stop_at": auto_stop_at,
    });
    if let Some(engine) = stt {
        body["stt"] = json!(engine);
    }
    (StatusCode::OK, Json(body)).into_response()
}

/// The `MeetingPatch` `start_meeting` applies after a successful start.
///
/// A first start (no prior `ended_at`) stamps `started_at = now`, replacing the row's
/// creation-time placeholder. A restart leaves `started_at` alone, since it feeds the
/// continuation-session clock (`meetings::session_offset_ms`), and clears the stale
/// `ended_at` so the next `/stop` stamps a fresh one.
///
/// `started_at = 0` is the schema's "unstarted" sentinel, but `MeetingRepo::create`
/// writes "now" instead, so a set `ended_at` is what actually marks a restart.
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

pub(crate) fn now_unix_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// `POST /api/meetings/:id/audio-device`: hot-swap the mic device on a recording meeting.
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

/// `POST /api/meetings/:id/mic-muted`: pause or resume the mic. `Channel::System` is untouched.
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

/// `POST /api/meetings/:id/echo` - open/close/hot-swap the mic echo.
/// An omitted field falls back to the persisted setting.
#[derive(Deserialize, Default)]
struct SetEchoRequest {
    enabled: Option<bool>,
    device: Option<String>,
}

async fn set_meeting_echo(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<SetEchoRequest>,
) -> impl IntoResponse {
    use crate::meetings::SwitchDeviceError;

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
    let enabled = body.enabled.unwrap_or(g.audio_echo_enabled);
    let device = body
        .device
        .clone()
        .unwrap_or_else(|| g.audio_echo_output_device.clone());

    match state
        .meetings
        .set_echo(&id, enabled, device.clone(), g.audio_echo_buffer)
        .await
    {
        Ok((live_enabled, live_device)) => {
            // Persist the live result, not the request - a failed open must not
            // record enabled: true.
            let persisted_device = if live_enabled {
                live_device.clone()
            } else {
                device
            };
            let patch = yogurt_db::settings::GeneralPatch {
                audio_echo_enabled: Some(live_enabled),
                audio_echo_output_device: Some(persisted_device.clone()),
                ..Default::default()
            };
            if let Err(e) = yogurt_db::settings::save_general_patch(&state.db, patch) {
                tracing::warn!(error = %e, "failed to persist echo settings after live apply");
            }
            // stop_echo's reply carries "" - use the persisted device instead.
            let response_device = if live_enabled {
                live_device
            } else {
                persisted_device
            };
            (
                StatusCode::OK,
                Json(json!({ "enabled": live_enabled, "device": response_device })),
            )
                .into_response()
        }
        Err(SwitchDeviceError::NotFound) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "meeting not found" })),
        )
            .into_response(),
        Err(SwitchDeviceError::NotRecording) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "meeting is not currently recording" })),
        )
            .into_response(),
        Err(SwitchDeviceError::Device(msg)) => {
            (StatusCode::BAD_REQUEST, Json(json!({ "error": msg }))).into_response()
        }
    }
}

/// Idempotent. Dropping the supervisor makes the audio thread drop the AudioStream,
/// which stops cpal + SCK.
async fn stop_meeting(State(state): State<AppState>, Path(id): Path<Uuid>) -> impl IntoResponse {
    match state.stop_meeting(&id).await {
        Ok(()) => (StatusCode::OK, Json(json!({ "status": "stopped" }))).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("{e:#}") })),
        )
            .into_response(),
    }
}

/// Dismisses the silence warning by treating the meeting as audible now.
async fn keep_recording(State(state): State<AppState>, Path(id): Path<Uuid>) -> impl IntoResponse {
    match state.meetings.get(&id).await {
        Some(m) => {
            m.last_audible_ms
                .store(now_unix_ms() as u64, std::sync::atomic::Ordering::Relaxed);
            (StatusCode::OK, Json(json!({ "status": "ok" }))).into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "meeting not found" })),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
struct AuthQuery {
    token: Option<String>,
}

/// Accepts `Authorization: Bearer <token>`, else `?token=<token>` (the WS convention).
/// Returns 403 on a missing or mismatched token and never logs the candidate.
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
