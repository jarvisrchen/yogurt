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
//! ## Auth
//!
//! This surface IS authenticated: `routes::router` mounts `stt_models_routes`
//! behind the same `require_session_token` middleware as `/api/settings*`
//! and the meeting REST surface. (An earlier revision of this comment
//! claimed the opposite — that was stale the moment `routes.rs` wrapped
//! this router in the session-token layer; fixed here so nobody re-derives
//! the wrong threat model from this file.)

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use serde::Serialize;
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};
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

/// Process-wide guard against two concurrent downloads of the same model —
/// e.g. a double-click on the Settings "Download" button, or a retried
/// request. Without this, two writers race on the same destination file
/// (interleaved `write_all` calls, one truncating while the other appends),
/// corrupting the in-progress download. Keyed by `ModelSpec::name` (a
/// `&'static str`, so no allocation is needed to check/insert).
fn in_flight_downloads() -> &'static Mutex<HashSet<&'static str>> {
    static SET: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
    SET.get_or_init(|| Mutex::new(HashSet::new()))
}

async fn start_download(
    State(s): State<AppState>,
    Path(name): Path<String>,
) -> Result<StatusCode, Error> {
    let spec = models::lookup(&name).ok_or(Error::NotFound)?;
    if !in_flight_downloads().lock().unwrap().insert(spec.name) {
        return Err(Error::Conflict(format!(
            "{} is already downloading",
            spec.name
        )));
    }
    // Spawn the download as a detached tokio task so the HTTP response
    // returns immediately (202 Accepted).  Progress + terminal state
    // are delivered over `/ws` via `AppState::app_events_tx`.
    let tx = s.app_events_tx.clone();
    let model_name = spec.name.to_string();
    tokio::spawn(async move {
        download_task(tx, model_name, spec).await;
        in_flight_downloads().lock().unwrap().remove(spec.name);
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
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(Error::Internal(format!("delete {path:?}: {e}"))),
    }
    // The sidecar `.sha256` marker (written by `is_downloaded`/
    // `download_to`) must go too — a stale marker left behind after
    // delete would let a subsequent corrupt/partial re-download pass
    // `is_downloaded_at`'s fast-path check against bytes that no longer
    // exist under it.
    let _ = std::fs::remove_file(models::marker_path(&path));
    Ok(StatusCode::NO_CONTENT)
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
    /// A download for this model is already in flight (see
    /// `in_flight_downloads`). Maps to 409 so the UI can show "already
    /// downloading" instead of silently starting a second writer.
    Conflict(String),
    Internal(String),
}

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        match self {
            Error::NotFound => (StatusCode::NOT_FOUND, "model not found").into_response(),
            Error::Conflict(s) => (StatusCode::CONFLICT, s).into_response(),
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

    /// A second `start_download` for the same model while one is already
    /// in flight must be rejected with 409 — a double-click on the
    /// Settings "Download" button must not spin up two writers on the
    /// same destination file. The first call's spawned download task
    /// will fail fast (no network in the test sandbox); either way the
    /// synchronous guard check runs before either request returns.
    #[tokio::test]
    async fn concurrent_downloads_of_the_same_model_are_rejected_with_409() {
        let tmp = tempfile::tempdir().unwrap();
        let storage = std::sync::Arc::new(
            crate::storage::Storage::init_at(&tmp.path().join("db.sqlite")).unwrap(),
        );
        let session = std::sync::Arc::new(
            crate::session::load_or_create(&tmp.path().join("session-token")).unwrap(),
        );
        let state = crate::state::AppState::in_memory(
            crate::Mode::Release,
            storage,
            session,
            7878,
            tmp.path().join("notes"),
        )
        .expect("in_memory state");

        // Belt-and-braces: this key is process-wide and other tests in this
        // binary don't touch "tiny.en", but start from a clean slate.
        in_flight_downloads().lock().unwrap().remove("tiny.en");

        let first = start_download(State(state.clone()), Path("tiny.en".to_string())).await;
        assert_eq!(
            first.unwrap(),
            StatusCode::ACCEPTED,
            "first request should be accepted"
        );

        let second = start_download(State(state.clone()), Path("tiny.en".to_string())).await;
        match second {
            Err(Error::Conflict(msg)) => assert!(
                msg.contains("tiny.en"),
                "conflict message should name the model: {msg}"
            ),
            other => panic!(
                "expected Error::Conflict for a concurrent duplicate download, got {other:?}"
            ),
        }

        // Cleanup so this test doesn't leak process-wide guard state into
        // any other test that happens to touch "tiny.en" in this binary.
        in_flight_downloads().lock().unwrap().remove("tiny.en");
    }
}
