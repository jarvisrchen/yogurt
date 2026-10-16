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

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/stt/models", get(list_models))
        .route("/api/stt/models/{name}/download", post(start_download))
        .route("/api/stt/models/{name}", delete(delete_model))
}

/// Mirrors `web/src/lib/api/stt.ts::ModelView`.
#[derive(Serialize)]
struct ModelView {
    name: String,
    size_mb: u32,
    downloaded: bool,
    intel_supported: bool,
    /// The verified copy lives in a Homebrew prefix, not yogurt's download dir.
    /// `delete_model` refuses to touch another tool's prefix.
    managed_by_homebrew: bool,
}

fn to_view(spec: &models::ModelSpec) -> ModelView {
    let resolved = models::resolve_model(spec);
    ModelView {
        name: spec.name.to_string(),
        size_mb: spec.size_mb,
        downloaded: resolved.is_some(),
        intel_supported: spec.intel_supported,
        managed_by_homebrew: resolved.is_some_and(|p| !models::is_user_owned(&p)),
    }
}

#[derive(Debug, Serialize)]
struct DeleteView {
    freed_bytes: u64,
}

async fn list_models(State(_s): State<AppState>) -> Json<Vec<ModelView>> {
    // `to_view` touches the filesystem and, on the legacy-marker migration, hashes a
    // multi-GB model file. That starves a tokio worker, so it runs on the blocking pool.
    let views =
        tokio::task::spawn_blocking(|| models::REGISTRY.iter().map(to_view).collect::<Vec<_>>())
            .await
            // JoinError only means a panic in a filesystem probe; don't 500 the picker over it.
            .unwrap_or_default();
    Json(views)
}

/// Guards against two concurrent downloads of one model, which would race
/// on the same destination file and corrupt it.
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
    // Detached so the response returns 202 immediately. Progress and terminal state go over `/ws`.
    let tx = s.app_events_tx.clone();
    let model_name = spec.name.to_string();
    tokio::spawn(async move {
        download_task(tx, model_name, spec).await;
        in_flight_downloads().lock().unwrap().remove(spec.name);
    });
    Ok(StatusCode::ACCEPTED)
}

async fn delete_model(
    State(s): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<DeleteView>, Error> {
    let spec = models::lookup(&name).ok_or(Error::NotFound)?;

    // Must run before any filesystem work so a 409 never touches disk.
    let general = yogurt_db::settings::load_general(&s.db)
        .map_err(|e| Error::Internal(format!("load general settings: {e}")))?;
    if general.stt_provider == "local" && general.stt_model == spec.name {
        return Err(Error::Conflict(format!(
            "{} is the active local model - switch to another model or to Cloud first",
            spec.name
        )));
    }

    // `model_path`'s legacy-marker migration can rename a multi-GB file, so stay off the tokio workers.
    tokio::task::spawn_blocking(move || -> Result<DeleteView, Error> {
        let path = models::model_path(spec)
            .map_err(|e| Error::Internal(format!("resolve model path: {e}")))?;
        // A Homebrew copy still satisfies `resolve_model`, so a no-op delete would
        // report "freed 0 bytes" while the model stays downloaded. Name the owner instead.
        // If both copies exist, deleting ours is correct.
        if !path.exists() {
            if let Some(external) = models::resolve_model(spec) {
                return Err(Error::Conflict(format!(
                    "{} is installed by Homebrew at {} - remove it with brew, \
                     not from Settings",
                    spec.name,
                    external.display()
                )));
            }
        }
        // Idempotent: an already-absent model reports `freed_bytes: 0`.
        let model_bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(Error::Internal(format!("delete {path:?}: {e}"))),
        }
        // A stale `.sha256` marker would let a later corrupt re-download pass
        // `is_downloaded_at`'s fast-path check. Best-effort.
        let marker_path = models::marker_path(&path);
        let marker_bytes = std::fs::metadata(&marker_path)
            .map(|m| m.len())
            .unwrap_or(0);
        let _ = std::fs::remove_file(&marker_path);
        Ok(DeleteView {
            freed_bytes: model_bytes + marker_bytes,
        })
    })
    .await
    .map_err(|e| Error::Internal(format!("delete task panicked: {e}")))?
    .map(Json)
}

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

#[derive(Debug)]
enum Error {
    NotFound,
    /// A download for this model is already in flight.
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

    /// The SPA model picker depends on these keys.
    #[test]
    fn model_view_serializes_with_expected_keys() {
        let spec = models::lookup("small.en").expect("small.en in REGISTRY");
        let view = to_view(spec);
        let json = serde_json::to_value(&view).unwrap();
        assert_eq!(json["name"], "small.en");
        assert!(json["size_mb"].as_u64().unwrap() > 0);
        assert!(json["downloaded"].is_boolean());
        assert!(json["intel_supported"].is_boolean());
        assert!(json["managed_by_homebrew"].is_boolean());
    }

    /// The first call's download task fails fast without network; the synchronous
    /// guard check runs before either request returns regardless.
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

        // The guard is process-wide, so start from a clean slate.
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

        in_flight_downloads().lock().unwrap().remove("tiny.en");
    }

    #[test]
    fn delete_view_serializes_freed_bytes() {
        let json = serde_json::to_value(DeleteView { freed_bytes: 123 }).unwrap();
        assert_eq!(json["freed_bytes"], 123);
    }

    /// The 409 must come before any filesystem access, so a bug here fails loudly
    /// instead of deleting from the developer's real `~/.yogurt/models/`.
    #[tokio::test]
    async fn deleting_the_active_local_model_is_rejected_with_409() {
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

        yogurt_db::settings::save_general_patch(
            &state.db,
            yogurt_db::settings::GeneralPatch {
                stt_provider: Some("local".to_string()),
                stt_model: Some("tiny.en".to_string()),
                ..Default::default()
            },
        )
        .expect("save_general_patch");

        let result = delete_model(State(state), Path("tiny.en".to_string())).await;
        match result {
            Err(Error::Conflict(msg)) => assert!(
                msg.contains("tiny.en"),
                "conflict message should name the model: {msg}"
            ),
            other => panic!(
                "expected Error::Conflict for deleting the active local model, got {other:?}"
            ),
        }
    }
}
