//! Create and patch write the markdown file through `MarkdownExporter` so it stays in sync with the row.
//! DELETE removes the file only with `?delete_file=true`, since the file is the user's grep-able source of truth.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::api::ApiError;
use crate::markdown_exporter::Meeting as ExpMeeting;
use crate::AppState;
use yogurt_db::{Meeting, MeetingPatch, NewMeeting};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/meetings/search", get(search))
        .route("/api/meetings", get(list).post(create))
        .route(
            "/api/meetings/{id}",
            get(get_one).patch(patch_one).delete(delete_one),
        )
        .route("/api/meetings/{id}/markdown", get(get_markdown))
        .route("/api/meetings/{id}/reveal", post(reveal_in_finder))
}

/// Every field is optional. An empty body gets the dated "Meeting 10/6/26" default.
#[derive(Debug, Deserialize, Default)]
pub struct CreateBody {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub started_at_unix_ms: Option<i64>,
    /// Fixture transcript segments in the shape of the `transcript_json` column.
    /// A raw `Value` so a bad shape fails as `ApiError::BadRequest` with an `{"error"}` body
    /// that `ctl` can surface, instead of axum's generic JSON rejection text.
    #[serde(default)]
    pub transcript_json: Option<serde_json::Value>,
    /// Stamp `ended_at` at creation so a `ctl meeting new` fixture is finished, not live.
    #[serde(default)]
    pub ended: Option<bool>,
    /// Fixture raw notes, so `ctl meeting new --notes-file` can stage an enhance input without a browser.
    #[serde(default)]
    pub notes_md: Option<String>,
}

/// One `transcript_json` entry. Same fields `ctl`'s transcript reader expects.
#[derive(Debug, Deserialize, Serialize)]
struct FixtureSegment {
    ts_ms: i64,
    channel: String,
    text: String,
}

/// Mirrors `yogurt_db::MeetingPatch` so an empty title can fall back to
/// "Untitled meeting" before the repo's validation rejects it.
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
    /// Replaces the label set. `None` leaves labels alone, `Some(vec![])` clears them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_ids: Option<Vec<String>>,
}

/// An empty `q` falls through to the chronological list. `limit` defaults to 50 and is capped at 200.
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
    let started_at = body
        .started_at_unix_ms
        .unwrap_or_else(crate::routes::now_unix_ms);
    let title = body
        .title
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("Meeting {}", yogurt_db::meetings::short_date(started_at)));

    // Validate before touching the DB so a malformed fixture never creates a stray row.
    let segments = body
        .transcript_json
        .map(|v| {
            serde_json::from_value::<Vec<FixtureSegment>>(v)
                .map_err(|e| ApiError::BadRequest(format!("invalid transcript_json: {e}")))
        })
        .transpose()?;
    let ended = body.ended.unwrap_or(false);
    let notes_md = body.notes_md;

    // The SQLite row must share the registry entry's UUID v7 id, or
    // `/api/meetings/:id/start` (which parses the id as `Uuid`) cannot find it.
    let live = s.meetings.create().await;
    let id = live.id.to_string();

    let new = NewMeeting {
        title,
        started_at_unix_ms: Some(started_at),
        id: Some(id.clone()),
    };
    let repo = s.meeting_repo.clone();
    let exporter = s.markdown_exporter.clone();
    let m = tokio::task::spawn_blocking(move || -> anyhow::Result<Meeting> {
        let mut m = repo.create(new)?;
        // No `stt_engine` is stamped: a fixture was never recorded.
        if segments.is_some() || ended || notes_md.is_some() {
            let transcript_json = segments.as_ref().map(serde_json::to_string).transpose()?;
            let ended_at = ended.then(|| {
                let last_ts_ms = segments
                    .as_ref()
                    .and_then(|s| s.iter().map(|seg| seg.ts_ms).max())
                    .unwrap_or(0);
                Some(m.started_at + last_ts_ms)
            });
            m = repo.patch(
                &m.id,
                MeetingPatch {
                    transcript_json,
                    ended_at,
                    notes_md,
                    ..Default::default()
                },
            )?;
        }
        // Writes the file with an empty body so Reveal in Finder works from day one.
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
    // Never let a PATCH blank a non-empty enriched document. Only enhance produces it,
    // so an empty overwrite is a client bug (a stale post-view mount flushing `""`).
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
    // Done here so the repo's empty-title rejection still protects other callers.
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
        llm_model: None,
        template: None,
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

/// Mirrors `AppState::patch_and_export`: the enriched body wins over raw notes.
fn exporter_view(m: &Meeting) -> ExpMeeting<'_> {
    ExpMeeting {
        id: &m.id,
        title: &m.title,
        started_at_unix_ms: m.started_at,
        ended_at_unix_ms: m.ended_at,
        body_md: m.enriched_md.as_deref().unwrap_or(&m.notes_md),
    }
}

/// Returns the on-disk markdown file, re-emitting it first if it is missing.
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

/// Reveals the markdown file in Finder (`open -R`). Non-macOS returns 204 without revealing.
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
        // `open -a Finder <path>` would open the file in the default editor, not reveal it.
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
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Default, Deserialize)]
pub struct DeleteQuery {
    /// Also unlink the markdown file. Opt-in so an absent flag never destroys a file.
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
    // `path_for` needs the row's fields, so resolve the path before deleting it.
    let removed = tokio::task::spawn_blocking(move || -> anyhow::Result<bool> {
        let path = if q.delete_file {
            repo.get(&id)?
                .map(|m| exporter.path_for(&exporter_view(&m)))
                .transpose()?
        } else {
            None
        };
        let removed = repo.delete(&id)?;
        // Unlink only after the row is gone. A missing file is fine since the exporter writes lazily.
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
