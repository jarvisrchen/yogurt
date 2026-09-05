//! HTTP surface for `yogurt-audio` capture.
//!
//! Four endpoints + one in-process hook point:
//!
//! - `GET /api/audio/permission` — surfaces both [`PermissionStatus`]
//!   fields (screen recording + microphone) for the Phase 7 onboarding
//!   flow (§5.10 / §5.11) and the parallel mic-permission flow added by
//!   quick task `260628-g71`. Polled by the UI while the user grants
//!   permissions in System Settings.
//! - `POST /api/audio/microphone/request` — fires the macOS microphone
//!   permission system dialog (idempotent — no-op if already granted /
//!   denied) and returns the *current* combined permission snapshot.
//!   Wired to the Welcome "Grant Microphone" button (DD-05).
//! - `POST /api/audio/screen-recording/request` — fires the macOS Screen
//!   Recording TCC prompt explicitly from the Welcome Step 1 button
//!   (quick task 260701-vjb). Previously the prompt only fired implicitly
//!   when `start_capture` opened an SCK stream, which deadlocked first-run
//!   users: the Welcome CTA is gated on the permission, so capture never
//!   started and the prompt never appeared.
//! - `GET /api/audio/devices` — enumerates input devices for the Phase 5
//!   settings dropdown (PRD §5.6).
//! - [`start_meeting_recording`] — Phase 3's hook into the audio pipeline.
//!   Called from `POST /api/meetings/:id/start` once that lands. Returns
//!   the `AudioStream` Phase 3 STT will fan-in via `tokio::select!`.
//!
//! [`AudioErrorWrap`] maps the typed [`AudioError`] variants onto HTTP
//! status codes — Phase 3 / Phase 7 handlers compose this into their own
//! responses by `?`-ing on the `AudioError` and letting the wrapper render
//! the JSON body.

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
};
use serde::{Deserialize, Serialize};
use yogurt_audio::{
    has_microphone_permission, has_screen_recording_permission, list_input_devices, play_test_tone,
    request_microphone_permission, request_screen_recording_permission, start_capture, AudioError,
    AudioStream, DeviceInfo, PermissionStatus,
};
use yogurt_db::settings;

use crate::state::AppState;

/// JSON body for `GET /api/audio/permission`,
/// `POST /api/audio/microphone/request`, and
/// `POST /api/audio/screen-recording/request`. All three endpoints return
/// the full snapshot — the SPA polls one cache key (`["audio", "permission"]`) and
/// derives both `useScreenRecordingStatus` and `useMicrophoneStatus` from
/// it via `select`-style accessors (DD-04).
///
/// Wire shape:
///
/// ```json
/// {
///   "screen_recording": "granted" | "denied" | "not_required",
///   "microphone":       "granted" | "denied" | "not_determined" | "not_required"
/// }
/// ```
///
/// Per DD-03 we extend the existing endpoint rather than adding a sibling
/// — the only consumer is the same-repo SPA shipped in the same binary,
/// so backward compatibility with the old `{status: …}` shape is moot.
#[derive(Serialize)]
pub struct PermissionResponse {
    pub screen_recording: PermissionStatus,
    pub microphone: PermissionStatus,
}

impl PermissionResponse {
    /// Read both TCC states in one go. Each call is a thread-safe
    /// idempotent query (CGPreflightScreenCaptureAccess +
    /// AVCaptureDevice.authorizationStatus), so we don't need to cache.
    fn snapshot() -> Self {
        Self {
            screen_recording: has_screen_recording_permission(),
            microphone: has_microphone_permission(),
        }
    }
}

/// `GET /api/audio/permission` — combined permission snapshot.
/// Phase 7 onboarding (and the parallel mic-permission flow) polls this
/// while the user grants permissions in System Settings; the response
/// drives both the §5.11 recovery card and the `<MicPermissionDenied />`
/// recovery card.
pub async fn get_permission() -> Json<PermissionResponse> {
    Json(PermissionResponse::snapshot())
}

/// `POST /api/audio/microphone/request` — fire the macOS microphone
/// permission system dialog. Idempotent: calling when already granted is
/// a cheap no-op; calling when already denied returns immediately without
/// re-prompting (macOS only ever asks the user once per app+TCC entry).
///
/// Returns the full combined snapshot (not just the mic field) so the SPA
/// can update both `useScreenRecordingStatus` and `useMicrophoneStatus`
/// in a single round-trip via `qc.invalidateQueries(permissionsKey)`.
pub async fn request_microphone() -> Json<PermissionResponse> {
    // Fire-and-forget. The dialog appears asynchronously; the SPA's 2s
    // polling loop picks up the eventual state change on the next tick.
    let _ = request_microphone_permission();
    Json(PermissionResponse::snapshot())
}

/// `POST /api/audio/screen-recording/request` — fire the macOS Screen
/// Recording TCC prompt if the app has never asked (quick task 260701-vjb).
/// If the user already denied, macOS will not re-prompt — the SPA offers a
/// System Settings deep link for that case instead.
///
/// Returns the full combined snapshot so the SPA updates both permission
/// hooks in one round-trip.
pub async fn request_screen_recording() -> Json<PermissionResponse> {
    // Fire-and-forget, mirroring `request_microphone`. The prompt appears
    // asynchronously; the SPA's 2s polling loop picks up the state change.
    let _ = request_screen_recording_permission();
    Json(PermissionResponse::snapshot())
}

/// `GET /api/audio/devices` — input devices for the §5.6 settings dropdown.
/// Returns a JSON array of `{ name, is_default, sample_rate }`.
pub async fn get_devices() -> Result<Json<Vec<DeviceInfo>>, (StatusCode, String)> {
    list_input_devices()
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

/// `GET /api/audio/output-devices` - output devices for the echo
/// destination picker. Same shape as `get_devices`.
pub async fn get_output_devices() -> Result<Json<Vec<DeviceInfo>>, (StatusCode, String)> {
    yogurt_audio::list_output_devices()
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

#[derive(Deserialize)]
pub struct TestEchoBody {
    #[serde(default)]
    device: Option<String>,
}

/// Result of `POST /api/audio/echo/test`. Same `{ok, model, error}` shape
/// as `TestProviderResult` in `api::settings` so the frontend's existing
/// verdict rendering (`TestKeyButton`) works unchanged; the two are
/// duplicated rather than shared because they live in separate modules
/// with no other coupling.
#[derive(Serialize)]
pub struct TestEchoResult {
    pub ok: bool,
    pub model: Option<String>,
    pub error: Option<String>,
}

/// `POST /api/audio/echo/test` - plays a 440 Hz tone on the echo output
/// device for 700 ms, independent of any recording or ScreenCaptureKit, so
/// the user can verify their echo routing (e.g. BlackHole) before a real
/// meeting. `device` missing/empty falls back to the persisted
/// `audio_echo_output_device` setting, then to the system default.
pub async fn test_echo(
    State(s): State<AppState>,
    Json(body): Json<TestEchoBody>,
) -> Result<Json<TestEchoResult>, (StatusCode, String)> {
    let general = settings::load_general(&s.db)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let device = body
        .device
        .filter(|d| !d.is_empty())
        .unwrap_or(general.audio_echo_output_device);
    let buffer = general.audio_echo_buffer;

    let result = tokio::task::spawn_blocking(move || {
        let dev = Some(device.as_str()).filter(|d| !d.is_empty());
        play_test_tone(dev, buffer)
    })
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("test task panicked: {e}"),
        )
    })?;

    Ok(Json(match result {
        Ok(name) => TestEchoResult {
            ok: true,
            model: Some(format!("{name} · 440 Hz for 0.7 s")),
            error: None,
        },
        Err(e) => TestEchoResult {
            ok: false,
            model: None,
            error: Some(e.to_string()),
        },
    }))
}

/// Begin recording. Phase 3 will wire this into `POST /api/meetings/:id/start`.
///
/// Surfaces [`AudioError::PermissionDenied`] for the §5.11 recovery flow.
/// The returned [`AudioStream`] holds RAII guards over both cpal and SCK
/// producers — dropping it terminates the OS-level capture handles
/// cleanly (AUDIO-06 / D-26).
pub fn start_meeting_recording(mic_device: Option<&str>) -> Result<AudioStream, AudioError> {
    start_capture(mic_device)
}

/// Newtype wrapper that gives [`AudioError`] an [`IntoResponse`] impl
/// without orphan-rule violation. Phase 3 handlers use `?` to propagate
/// audio failures into the response body without case-by-case mapping.
pub struct AudioErrorWrap(pub AudioError);

impl From<AudioError> for AudioErrorWrap {
    fn from(e: AudioError) -> Self {
        Self(e)
    }
}

impl IntoResponse for AudioErrorWrap {
    fn into_response(self) -> axum::response::Response {
        let (status, body) = match self.0 {
            AudioError::PermissionDenied => (
                StatusCode::FORBIDDEN,
                serde_json::json!({
                    "error": "permission_denied",
                    "message": "macOS Screen Recording permission is required",
                    "recovery": "open System Settings → Privacy & Security → Screen Recording",
                }),
            ),
            AudioError::UnsupportedPlatform => (
                StatusCode::NOT_IMPLEMENTED,
                serde_json::json!({
                    "error": "unsupported_platform",
                    "message": "system audio capture requires macOS 13+",
                }),
            ),
            other => (
                StatusCode::INTERNAL_SERVER_ERROR,
                serde_json::json!({ "error": "audio", "message": other.to_string() }),
            ),
        };
        (status, Json(body)).into_response()
    }
}
