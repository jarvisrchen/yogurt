//! HTTP surface for `yogurt-audio` capture.
//!
//! Three endpoints + one in-process hook point:
//!
//! - `GET /api/audio/permission` — surfaces both [`PermissionStatus`]
//!   fields (screen recording + microphone) for the Phase 7 onboarding
//!   flow (§5.10 / §5.11) and the parallel mic-permission flow added by
//!   quick task `260628-g71`. Polled by the UI while the user grants
//!   permissions in System Settings.
//! - `POST /api/audio/microphone/request` — fires the macOS microphone
//!   permission system dialog (idempotent — no-op if already granted /
//!   denied) and returns the *current* combined permission snapshot.
//!   Wired to the Welcome "Grant Microphone" button (DD-05). Note that
//!   screen-recording permission is requested implicitly the first time
//!   `start_capture` opens an SCK stream; we do not currently expose a
//!   parallel `POST /api/audio/screen-recording/request` because the
//!   existing CG-based detection path triggers the prompt as a side
//!   effect of `start_capture` already.
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
    http::StatusCode,
    response::{IntoResponse, Json},
};
use serde::Serialize;
use yogurt_audio::{
    has_microphone_permission, has_screen_recording_permission, list_input_devices,
    request_microphone_permission, start_capture, AudioError, AudioStream, DeviceInfo,
    PermissionStatus,
};

/// JSON body for `GET /api/audio/permission` and
/// `POST /api/audio/microphone/request`. Both endpoints return the full
/// snapshot — the SPA polls one cache key (`["audio", "permission"]`) and
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

/// `GET /api/audio/devices` — input devices for the §5.6 settings dropdown.
/// Returns a JSON array of `{ name, is_default, sample_rate }`.
pub async fn get_devices() -> Result<Json<Vec<DeviceInfo>>, (StatusCode, String)> {
    list_input_devices()
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

/// Begin recording. Phase 3 will wire this into `POST /api/meetings/:id/start`.
///
/// Surfaces [`AudioError::PermissionDenied`] for the §5.11 recovery flow.
/// The returned [`AudioStream`] holds RAII guards over both cpal and SCK
/// producers — dropping it terminates the OS-level capture handles
/// cleanly (AUDIO-06 / D-26).
pub fn start_meeting_recording() -> Result<AudioStream, AudioError> {
    start_capture()
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
