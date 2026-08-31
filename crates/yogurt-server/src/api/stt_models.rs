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
//! `DELETE` returns `200 OK` with `{"freed_bytes": <u64>}` on success
//! (idempotent - deleting an already-absent model returns `freed_bytes: 0`,
//! not an error), or `409 Conflict` if `name` is the currently active
//! local model (`general.stt_provider == "local" && general.stt_model ==
//! name`) - the user must switch models/providers before deleting the one
//! in use.
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
    /// `true` when the verified copy lives in a Homebrew prefix rather
    /// than yogurt's own download dir (AUD-4). The picker swaps the
    /// trash affordance for a "brew" chip, because `delete_model`
    /// refuses to reach into another tool's prefix.
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

/// Response body for a successful `DELETE /api/stt/models/{name}`.
#[derive(Debug, Serialize)]
struct DeleteView {
    freed_bytes: u64,
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
    State(s): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<DeleteView>, Error> {
    let spec = models::lookup(&name).ok_or(Error::NotFound)?;

    // Refuse to delete the model actively backing local STT - this check
    // must run before any filesystem work so a test (or a real request)
    // never touches disk when it's going to 409 anyway.
    let general = yogurt_db::settings::load_general(&s.db)
        .map_err(|e| Error::Internal(format!("load general settings: {e}")))?;
    if general.stt_provider == "local" && general.stt_model == spec.name {
        return Err(Error::Conflict(format!(
            "{} is the active local model - switch to another model or to Cloud first",
            spec.name
        )));
    }

    // Filesystem work (in particular `model_path`'s one-time legacy-marker
    // migration, which can rename a multi-GB file) must not run on a tokio
    // worker - same reasoning as `list_models` above.
    tokio::task::spawn_blocking(move || -> Result<DeleteView, Error> {
        let path = models::model_path(spec)
            .map_err(|e| Error::Internal(format!("resolve model path: {e}")))?;
        // AUD-4: with nothing of ours to delete, a Homebrew-installed
        // copy would still satisfy `resolve_model`, so a plain no-op
        // DELETE would report "freed 0 bytes" and leave the picker
        // showing the model as downloaded. Say who owns it instead.
        // (If BOTH copies exist, deleting ours is correct and the
        // Homebrew one legitimately keeps the model available.)
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
        // `metadata` before `remove_file` so a model that was already
        // absent reports `freed_bytes: 0` rather than erroring - DELETE
        // stays idempotent.
        let model_bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(Error::Internal(format!("delete {path:?}: {e}"))),
        }
        // The sidecar `.sha256` marker (written by `is_downloaded`/
        // `download_to`) must go too — a stale marker left behind after
        // delete would let a subsequent corrupt/partial re-download pass
        // `is_downloaded_at`'s fast-path check against bytes that no longer
        // exist under it. Best-effort, like the removal below it.
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
    /// `intel_supported`, `managed_by_homebrew` - any rename here
    /// silently breaks the Settings page model picker.
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

    /// Wire-shape gate for the DELETE 200 body — a rename here silently
    /// breaks whatever the Settings page does with the freed-space number.
    #[test]
    fn delete_view_serializes_freed_bytes() {
        let json = serde_json::to_value(DeleteView { freed_bytes: 123 }).unwrap();
        assert_eq!(json["freed_bytes"], 123);
    }

    /// Deleting the model currently backing local STT must 409 *before*
    /// touching the filesystem — the 409 check runs ahead of any fs access,
    /// which this test relies on: the in-memory state's home dir is never
    /// created, so a filesystem-touching bug here would fail loudly rather
    /// than silently deleting from the developer's real
    /// `~/.yogurt/models/`.
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
