pub mod chat;
pub mod labels;
pub mod meetings;
pub mod settings;
pub mod stt_models;

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

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
        // "label not found" is 400 here because meeting PATCH treats an unknown `label_ids`
        // entry as a bad request. The labels router maps its own unknown id to 404 first.
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
