//! Phase 8 (Plan 08-03): REST surface for whisper.cpp model management.
//!
//! Mounted under `/api/stt/*`:
//!
//! | Method | Path                                  | Handler         |
//! |--------|---------------------------------------|-----------------|
//! | GET    | `/api/stt/models`                     | `list_models`   |
//! | POST   | `/api/stt/models/{name}/download`     | `start_download`|
//! | DELETE | `/api/stt/models/{name}`              | `delete_model`  |
//!
//! The download endpoint is **202-Accepted-and-fire-and-forget** —
//! it spawns a background task that calls
//! [`yogurt_stt::models::download`] with a progress callback that fans
//! out [`crate::ws::WsEvent::SttModelDownloadProgress`] over the
//! app-wide event channel. Terminal state (`Complete` / `Error`)
//! arrives on the same WS surface.  The Settings page's
//! `useModelDownloadProgress` hook subscribes once on mount.
//!
//! ## Why not auth-gate this surface?
//!
//! The Phase 5 `/api/settings*` routes intentionally skipped the
//! session-token middleware (see `routes::router` comments) so the
//! load-bearing security tests (`tests/settings_api.rs::api_responses_*`)
//! can hit them with a plain reqwest client.  This module follows the
//! same convention — the surface is bound to 127.0.0.1 only, and the
//! handlers never touch secrets.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use serde::Serialize;
use yogurt_stt::models;

use crate::state::AppState;
use crate::ws;

/// Build the `Router<AppState>` containing every `/api/stt/*` route.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/stt/models", get(list_models))
        .route("/api/stt/models/{name}/download", post(start_download))
        .route("/api/stt/models/{name}", delete(delete_model))
}

// ─── Wire format ────────────────────────────────────────────────────────────

/// View of one model as exposed to the SPA.  Mirrors
/// `web/src/lib/api/stt.ts::ModelView`.
#[derive(Serialize)]
struct ModelView {
    name: String,
    size_mb: u32,
    downloaded: bool,
    intel_supported: bool,
}

fn to_view(spec: &models::ModelSpec) -> ModelView {
    ModelView {
        name: spec.name.to_string(),
        size_mb: spec.size_mb,
        downloaded: models::is_downloaded(spec),
        intel_supported: spec.intel_supported,
    }
}

// ─── Handlers ───────────────────────────────────────────────────────────────

async fn list_models(State(_s): State<AppState>) -> Json<Vec<ModelView>> {
    // `to_view` -> `is_downloaded` touches the filesystem and, on the
    // one-time legacy-marker migration, hashes a multi-GB model file.
    // That must never run on a tokio worker (it starved the runtime,
    // quick-260701-wjs), so the whole registry->view mapping goes onto
    // the blocking pool. REGISTRY is &'static, no captures needed.
    let views =
        tokio::task::spawn_blocking(|| models::REGISTRY.iter().map(to_view).collect::<Vec<_>>())
            .await
            // JoinError only means the closure panicked; don't 500 the model
            // picker over a panic in a filesystem probe.
            .unwrap_or_default();
    Json(views)
}

async fn start_download(
    State(s): State<AppState>,
    Path(name): Path<String>,
) -> Result<StatusCode, Error> {
    let spec = models::lookup(&name).ok_or(Error::NotFound)?;
    // Spawn the download as a detached tokio task so the HTTP response
    // returns immediately (202 Accepted).  Progress + terminal state
    // are delivered over `/ws` via `AppState::app_events_tx`.
    let tx = s.app_events_tx.clone();
    let model_name = spec.name.to_string();
    tokio::spawn(async move {
        download_task(tx, model_name, spec).await;
    });
    Ok(StatusCode::ACCEPTED)
}

async fn delete_model(
    State(_s): State<AppState>,
    Path(name): Path<String>,
) -> Result<StatusCode, Error> {
    let spec = models::lookup(&name).ok_or(Error::NotFound)?;
    let path = models::model_path(spec)
        .map_err(|e| Error::Internal(format!("resolve model path: {e}")))?;
    // `remove_file` is fine if the file is absent — treat NotFound as
    // already-deleted so DELETE is idempotent.
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(StatusCode::NO_CONTENT),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(StatusCode::NO_CONTENT),
        Err(e) => Err(Error::Internal(format!("delete {path:?}: {e}"))),
    }
}

// ─── Background download task ──────────────────────────────────────────────

/// The spawned worker that runs one model download and emits progress
/// + terminal events on the app-wide event channel.
async fn download_task(tx: ws::AppEventTx, model_name: String, spec: &'static models::ModelSpec) {
    let tx_progress = tx.clone();
    let model_name_progress = model_name.clone();
    let result = models::download(spec, move |p| {
        ws::send_stt_model_download_progress(
            &tx_progress,
            &model_name_progress,
            p.bytes_downloaded,
            p.total_bytes,
            p.bytes_per_sec,
            p.eta_seconds,
        );
    })
    .await;

    match result {
        Ok(()) => ws::send_stt_model_download_complete(&tx, &model_name),
        Err(e) => ws::send_stt_model_download_error(&tx, &model_name, &format!("{e}")),
    }
}

// ─── Error mapping ──────────────────────────────────────────────────────────

#[derive(Debug)]
enum Error {
    NotFound,
    Internal(String),
}

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        match self {
            Error::NotFound => (StatusCode::NOT_FOUND, "model not found").into_response(),
            Error::Internal(s) => (StatusCode::INTERNAL_SERVER_ERROR, s).into_response(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Wire-shape gate: the SPA expects `name`, `size_mb`, `downloaded`,
    /// `intel_supported` — any rename here silently breaks the
    /// Settings page model picker.
    #[test]
    fn model_view_serializes_with_expected_keys() {
        let spec = models::lookup("small.en").expect("small.en in REGISTRY");
        let view = to_view(spec);
        let json = serde_json::to_value(&view).unwrap();
        assert_eq!(json["name"], "small.en");
        assert!(json["size_mb"].as_u64().unwrap() > 0);
        assert!(json["downloaded"].is_boolean());
        assert!(json["intel_supported"].is_boolean());
    }
}
