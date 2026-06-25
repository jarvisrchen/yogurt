//! HTTP surface for `yogurt-audio` capture.
//!
//! Two GET endpoints + one in-process hook point:
//!
//! - `GET /api/audio/permission` — surfaces [`PermissionStatus`] for the
//!   Phase 7 onboarding flow (§5.10 / §5.11). Polled by the UI while the
//!   user grants Screen Recording in System Settings.
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
    has_screen_recording_permission, list_input_devices, start_capture, AudioError, AudioStream,
    DeviceInfo, PermissionStatus,
};

/// JSON body for `GET /api/audio/permission`. The single-field shape
/// keeps the response forward-compatible — Phase 7 can add e.g.
/// `last_changed_at` without breaking existing clients.
#[derive(Serialize)]
pub struct PermissionResponse {
    pub status: PermissionStatus,
}

/// `GET /api/audio/permission` — Screen Recording permission state.
/// Phase 7 onboarding polls this while the user grants permission in
/// System Settings; the response drives the §5.11 recovery card.
pub async fn get_permission() -> Json<PermissionResponse> {
    Json(PermissionResponse {
        status: has_screen_recording_permission(),
    })
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
