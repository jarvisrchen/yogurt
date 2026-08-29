//! Phase 5 (Plan 05-03) HTTP API module — currently just the `settings`
//! surface (`/api/settings*`). Phase 4's `enhance` and `meetings` handlers
//! remain at their existing `crates/yogurt-server/src/{enhance,routes}.rs`
//! locations; this module is the entry point for *new* API surfaces added
//! after Phase 4.

pub mod chat;
// Phase 7 (Plan 07-01): Library REST surface — CRUD over the SQLite-backed
// `MeetingRepo`. Coexists with the Phase 3 `POST /api/meetings`
// (`routes::create_meeting`) which still creates the in-memory streaming
// `Registry` entry; Phase 7's handlers operate on the persisted directory
// instead.
pub mod labels;
pub mod meetings;
pub mod settings;
// Phase 8 (Plan 08-03): whisper.cpp model management REST surface.
// Routes mount under `/api/stt/*` and spawn background download tasks
// that emit `stt_model_download_*` events on the app-wide `/ws` channel.
pub mod stt_models;

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

/// Centralized error → response mapping shared by `api::meetings` and
/// `api::labels`. The error message bubbles through the response body so
/// REST consumers can surface the underlying problem; the wire shape
/// matches the existing `routes::*` handlers' `{"error": "<msg>"}` envelope.
#[derive(Debug)]
pub(crate) enum ApiError {
    NotFound,
    BadRequest(String),
    Internal(anyhow::Error),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (code, msg) = match self {
            ApiError::NotFound => (StatusCode::NOT_FOUND, "not found".to_string()),
            ApiError::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            ApiError::Internal(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}")),
        };
        (code, Json(json!({ "error": msg }))).into_response()
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        // Map specific repo-bail strings onto REST status codes so
        // PATCH / DELETE behave as clients expect. "label not found" maps
        // to 400 (not 404) here because this generic conversion is used by
        // the meetings PATCH path, where an unknown `label_ids` entry is a
        // bad-request against an otherwise-existing meeting. The labels
        // router's own PATCH/DELETE handlers special-case their own
        // "unknown label id" outcome to 404 before falling through here —
        // see `api::labels`.
        let s = e.to_string();
        if s.contains("label not found")
            || s.contains("invalid color")
            || s.contains("already exists")
            || s.contains("empty")
            || s.contains("40 characters")
        {
            ApiError::BadRequest(s)
        } else if s.contains("not found") {
            ApiError::NotFound
        } else {
            ApiError::Internal(e)
        }
    }
}
