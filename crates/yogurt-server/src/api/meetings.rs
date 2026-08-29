//! Phase 7 (Plan 07-01) — Library REST surface.
//!
//! Endpoints:
//!
//! | Method  | Path                  | Purpose                                  |
//! |---------|-----------------------|------------------------------------------|
//! | GET     | `/api/meetings`       | List all meetings newest-first.          |
//! | POST    | `/api/meetings`       | Create a new meeting (Library + stream). |
//! | GET     | `/api/meetings/:id`   | Fetch one meeting.                       |
//! | PATCH   | `/api/meetings/:id`   | Update title / notes / starred / etc.    |
//! | DELETE  | `/api/meetings/:id`   | Remove SQLite row + cascade chat rows.   |
//!
//! **Auth:** every route is mounted behind `routes::require_session_token`
//! by `routes::router`. Handlers here don't repeat that check.
//!
//! **Markdown side-effect:** create + patch funnel through
//! `MarkdownExporter::write` so the canonical `~/.yogurt/notes/<…>.md` file
//! is always in sync with the SQLite row. DELETE removes the markdown file
//! only when the caller passes `?delete_file=true`; the default stays false
//! so the file remains the user's grep-able source of truth (D-10 / PRD
//! §5.7) unless deletion is explicitly asked for. The Library UI checkbox
//! defaults to checked, so the common path does delete both.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;

use crate::api::ApiError;
use crate::markdown_exporter::Meeting as ExpMeeting;
use crate::AppState;
use yogurt_db::{Meeting, MeetingPatch, NewMeeting};

/// Build the Library REST router. Mounted in `routes::router` *behind* the
/// session-token middleware; this function only returns the raw route table
/// so the caller can compose auth + tracing layers around it.
pub fn router() -> Router<AppState> {
    Router::new()
        // Phase 7 Plan 07-02: register `/api/meetings/search` BEFORE the
        // `/api/meetings/{id}` route so the axum 0.8 matcher dispatches
        // literal-path matches ahead of the path-param fallback. Axum 0.8
        // does the right thing here regardless of registration order
        // (literals beat params), but the explicit ordering keeps the
        // intent legible at the route table.
        .route("/api/meetings/search", get(search))
        .route("/api/meetings", get(list).post(create))
        .route(
            "/api/meetings/{id}",
            get(get_one).patch(patch_one).delete(delete_one),
        )
        // Phase 7 Plan 07-03 — per-meeting markdown affordances.
        // `GET /:id/markdown` returns the on-disk Phase-4 MarkdownExporter
        // file (front-matter + body) so the kebab-menu "Copy markdown"
        // copies the exact bytes the user would see if they opened the
        // file in Finder. `POST /:id/reveal` shells out to `open -R` on
        // macOS — registered as POST (not GET) because it has an
        // observable side-effect (Finder activation).
        .route("/api/meetings/{id}/markdown", get(get_markdown))
        .route("/api/meetings/{id}/reveal", post(reveal_in_finder))
}

/// Body for `POST /api/meetings`. Both fields are optional — the Library
/// sidebar's "+ New meeting" button sends an empty body and gets the
/// "Untitled meeting" default.
#[derive(Debug, Deserialize, Default)]
pub struct CreateBody {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub started_at_unix_ms: Option<i64>,
}

/// Body for `PATCH /api/meetings/:id`. Mirrors `yogurt_db::MeetingPatch`
/// but exposed at this layer so we can normalize empty-title inputs to
/// the "Untitled meeting" fallback before the repo's stricter validation
/// rejects them (LIB-08 inline-rename UX).
#[derive(Debug, Deserialize, Default)]
pub struct PatchBody {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes_md: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript_json: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enriched_md: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<Option<i64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub starred: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stt_engine: Option<String>,
    /// Replace this meeting's label set with exactly these ids. `None`
    /// leaves labels alone; `Some(vec![])` clears them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_ids: Option<Vec<String>>,
}

// ─── Handlers ──────────────────────────────────────────────────────────────

/// Query string for `GET /api/meetings/search`. `q` is the user's
/// free-text query (empty falls through to the chronological list);
/// `limit` caps the result count at the server side (defaults to 50,
/// hard-capped at 200 to keep payloads bounded even if a script asks
/// for more).
#[derive(Debug, Deserialize)]
struct SearchQuery {
    q: String,
    #[serde(default)]
    limit: Option<usize>,
}

async fn search(
    State(s): State<AppState>,
    axum::extract::Query(qs): axum::extract::Query<SearchQuery>,
) -> Result<Json<Vec<Meeting>>, ApiError> {
    let limit = qs.limit.unwrap_or(50).min(200);
    let repo = s.meeting_repo.clone();
    let xs = tokio::task::spawn_blocking(move || repo.search(&qs.q, limit))
        .await
        .map_err(|e| ApiError::Internal(anyhow::Error::new(e)))?
        .map_err(ApiError::from)?;
    Ok(Json(xs))
}

async fn list(State(s): State<AppState>) -> Result<Json<Vec<Meeting>>, ApiError> {
    let repo = s.meeting_repo.clone();
    let xs = tokio::task::spawn_blocking(move || repo.list())
        .await
        .map_err(|e| ApiError::Internal(anyhow::Error::new(e)))?
        .map_err(ApiError::from)?;
    Ok(Json(xs))
}

async fn create(
    State(s): State<AppState>,
    body: Option<Json<CreateBody>>,
) -> Result<(StatusCode, Json<Meeting>), ApiError> {
    let body = body.map(|Json(b)| b).unwrap_or_default();
    let title = body
        .title
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .unwrap_or("Untitled meeting")
        .to_string();

    // Bootstrap an in-memory streaming Meeting first so the SQLite row
    // shares the same id (UUID v7) as the registry entry. Without this
    // alignment, the existing `/api/meetings/:id/start` route — which
    // parses the path param as `Uuid` — would never find a freshly
    // created Library meeting.
    let live = s.meetings.create().await;
    let id = live.id.to_string();

    let new = NewMeeting {
        title,
        started_at_unix_ms: body.started_at_unix_ms,
        id: Some(id.clone()),
    };
    let repo = s.meeting_repo.clone();
    let exporter = s.markdown_exporter.clone();
    let m = tokio::task::spawn_blocking(move || -> anyhow::Result<Meeting> {
        let m = repo.create(new)?;
        // Side-effect: write the canonical markdown file. The body is
        // empty until the user types into the notes editor — Phase 4
        // MarkdownExporter still emits the YAML front-matter envelope
        // so the file exists on disk from day one (Reveal-in-Finder
        // from a never-edited meeting still works).
        exporter.write(&ExpMeeting {
            id: &m.id,
            title: &m.title,
            started_at_unix_ms: m.started_at,
            ended_at_unix_ms: m.ended_at,
            body_md: m.enriched_md.as_deref().unwrap_or(&m.notes_md),
        })?;
        Ok(m)
    })
    .await
    .map_err(|e| ApiError::Internal(anyhow::Error::new(e)))?
    .map_err(ApiError::from)?;

    Ok((StatusCode::CREATED, Json(m)))
}

async fn get_one(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Meeting>, ApiError> {
    let repo = s.meeting_repo.clone();
    let opt = tokio::task::spawn_blocking(move || repo.get(&id))
        .await
        .map_err(|e| ApiError::Internal(anyhow::Error::new(e)))?
        .map_err(ApiError::from)?;
    opt.map(Json).ok_or(ApiError::NotFound)
}

async fn patch_one(
    State(s): State<AppState>,
    Path(id): Path<String>,
    body: Option<Json<PatchBody>>,
) -> Result<Json<Meeting>, ApiError> {
    let mut body = body.map(|Json(b)| b).unwrap_or_default();
    // DATA-LOSS GUARD (defense-in-depth; the client autosave has its own):
    // never let a PATCH replace a non-empty enriched document with an
    // empty one. The enriched body is only ever produced by enhance — an
    // empty overwrite is always a client bug (observed live: a stale
    // post-view mount flushing `""` on unmount), never user intent. The
    // user-intent path for removing content is deleting the meeting.
    if matches!(&body.enriched_md, Some(Some(md)) if md.trim().is_empty()) {
        let repo = s.meeting_repo.clone();
        let id_probe = id.clone();
        let stored = tokio::task::spawn_blocking(move || repo.get(&id_probe))
            .await
            .map_err(|e| ApiError::Internal(anyhow::Error::new(e)))?
            .map_err(ApiError::from)?;
        if stored
            .as_ref()
            .and_then(|m| m.enriched_md.as_deref())
            .is_some_and(|md| !md.trim().is_empty())
        {
            tracing::warn!(
                meeting_id = %id,
                "rejected PATCH that would blank a non-empty enriched_md"
            );
            body.enriched_md = None;
        }
    }
    // LIB-08: empty-title → "Untitled meeting" fallback. Done at the API
    // layer so the repo's empty-title rejection still protects the DB
    // invariant for non-Library callers.
    let title = body.title.as_deref().map(|t| {
        let t = t.trim();
        if t.is_empty() {
            "Untitled meeting".to_string()
        } else {
            t.to_string()
        }
    });
    let patch = MeetingPatch {
        title,
        started_at: body.started_at,
        notes_md: body.notes_md,
        transcript_json: body.transcript_json,
        enriched_md: body.enriched_md,
        ended_at: body.ended_at,
        starred: body.starred,
        stt_engine: body.stt_engine,
        label_ids: body.label_ids,
    };
    let state_for_blocking = s.clone();
    let id_for_blocking = id.clone();
    let m = tokio::task::spawn_blocking(move || {
        state_for_blocking.patch_and_export(&id_for_blocking, patch)
    })
    .await
    .map_err(|e| ApiError::Internal(anyhow::Error::new(e)))?
    .map_err(ApiError::from)?;
    Ok(Json(m))
}

/// Build the borrowed `ExpMeeting` view the `MarkdownExporter` accepts.
/// Mirrors `AppState::patch_and_export` — prefer the enriched body over
/// raw notes when both exist, so the on-disk file represents the latest
/// LLM-augmented output (matches Phase 4 §5.7 invariant).
fn exporter_view(m: &Meeting) -> ExpMeeting<'_> {
    ExpMeeting {
        id: &m.id,
        title: &m.title,
        started_at_unix_ms: m.started_at,
        ended_at_unix_ms: m.ended_at,
        body_md: m.enriched_md.as_deref().unwrap_or(&m.notes_md),
    }
}

/// `GET /api/meetings/{id}/markdown` — return the on-disk Phase-4
/// MarkdownExporter file for this meeting as `text/markdown`. If the
/// file is missing (hand-deleted, fresh meeting that never funneled
/// through the exporter), we lazily re-emit it via `MarkdownExporter::write`
/// so the kebab-menu Copy never silently returns stale content.
async fn get_markdown(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<(axum::http::HeaderMap, String), ApiError> {
    let repo = s.meeting_repo.clone();
    let exporter = s.markdown_exporter.clone();
    let id_for_blocking = id.clone();
    let body = tokio::task::spawn_blocking(move || -> anyhow::Result<String> {
        let m = repo
            .get(&id_for_blocking)?
            .ok_or_else(|| anyhow::anyhow!("meeting not found"))?;
        let path = exporter.path_for(&exporter_view(&m))?;
        // Idempotent: write if missing (covers the rare case where the
        // user hand-deleted the file from ~/.yogurt/notes/ but the
        // SQLite row survived).
        if !path.exists() {
            exporter.write(&exporter_view(&m))?;
        }
        Ok(std::fs::read_to_string(&path)?)
    })
    .await
    .map_err(|e| ApiError::Internal(anyhow::Error::new(e)))?
    .map_err(ApiError::from)?;

    let mut h = axum::http::HeaderMap::new();
    h.insert(
        axum::http::header::CONTENT_TYPE,
        "text/markdown; charset=utf-8"
            .parse()
            .expect("static content-type header value"),
    );
    Ok((h, body))
}

/// `POST /api/meetings/{id}/reveal` — open Finder with the on-disk
/// markdown file selected (`open -R <path>`). 204 on success. macOS-only
/// per project constraints; on non-macOS the endpoint still returns
/// 204 after locating/writing the file (no-op shell out is cleaner than
/// a 501 for cross-platform devs running the test suite).
async fn reveal_in_finder(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let repo = s.meeting_repo.clone();
    let exporter = s.markdown_exporter.clone();
    let id_for_blocking = id.clone();
    let path = tokio::task::spawn_blocking(move || -> anyhow::Result<std::path::PathBuf> {
        let m = repo
            .get(&id_for_blocking)?
            .ok_or_else(|| anyhow::anyhow!("meeting not found"))?;
        let path = exporter.path_for(&exporter_view(&m))?;
        if !path.exists() {
            exporter.write(&exporter_view(&m))?;
        }
        Ok(path)
    })
    .await
    .map_err(|e| ApiError::Internal(anyhow::Error::new(e)))?
    .map_err(ApiError::from)?;

    #[cfg(target_os = "macos")]
    {
        // `open -R <path>` reveals (and selects) the file in Finder.
        // We deliberately don't pipe through `open -a Finder <path>` —
        // that would *open* the file in the default editor, not reveal
        // it in Finder. Shell out off the tokio reactor via
        // spawn_blocking; `open` returns quickly but the syscall is
        // technically blocking.
        let status = tokio::task::spawn_blocking(move || {
            std::process::Command::new("open")
                .arg("-R")
                .arg(&path)
                .status()
        })
        .await
        .map_err(|e| ApiError::Internal(anyhow::Error::new(e)))?
        .map_err(|e| ApiError::Internal(anyhow::Error::new(e)))?;
        if !status.success() {
            return Err(ApiError::Internal(anyhow::anyhow!(
                "open -R failed: {status}"
            )));
        }
    }
    // On non-macOS we intentionally swallow the reveal — the path
    // exists, the SQLite row exists, and there's no Finder to talk to.
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Query string for `DELETE /api/meetings/:id`.
#[derive(Debug, Default, Deserialize)]
pub struct DeleteQuery {
    /// Also unlink `~/.yogurt/notes/<…>.md`. Defaults to false: an absent
    /// flag must never destroy a file, so the destructive behaviour is
    /// opt-in at the API layer even though the UI checkbox pre-checks it.
    #[serde(default)]
    delete_file: bool,
}

async fn delete_one(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<DeleteQuery>,
) -> Result<StatusCode, ApiError> {
    let repo = s.meeting_repo.clone();
    let exporter = s.markdown_exporter.clone();
    // Resolve the markdown path BEFORE dropping the row — `path_for` needs
    // the meeting's fields to rebuild the filename, and after the delete
    // there's nothing left to derive it from.
    let removed = tokio::task::spawn_blocking(move || -> anyhow::Result<bool> {
        let path = if q.delete_file {
            repo.get(&id)?
                .map(|m| exporter.path_for(&exporter_view(&m)))
                .transpose()?
        } else {
            None
        };
        // SQLite cascade purges the chat rows.
        let removed = repo.delete(&id)?;
        // Only unlink once the row is actually gone, so a failed delete
        // never leaves the library pointing at a missing file. A missing
        // file is not an error — the exporter writes lazily, so a meeting
        // that was never enhanced has no .md to remove.
        if removed {
            if let Some(path) = path {
                match std::fs::remove_file(&path) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => {
                        tracing::warn!(?e, path = %path.display(), "delete: could not remove markdown file");
                    }
                }
            }
        }
        Ok(removed)
    })
    .await
    .map_err(|e| ApiError::Internal(anyhow::Error::new(e)))?
    .map_err(ApiError::from)?;
    if removed {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}
