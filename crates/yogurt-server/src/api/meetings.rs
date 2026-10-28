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
use yogurt_db::{Meeting, MeetingFilter, MeetingPage, MeetingPatch, NewMeeting};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/meetings/search", get(search))
        .route("/api/meetings/archive", post(archive_before))
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
    pub archived: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stt_engine: Option<String>,
    /// Replaces the label set. `None` leaves labels alone, `Some(vec![])` clears them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_ids: Option<Vec<String>>,
}

const DEFAULT_LIMIT: usize = 25;
const MAX_LIMIT: usize = 100;

/// Paging and filters shared by the list and search routes. `starred=false` is the same as leaving it out.
/// `archived=true` lists only archived meetings, which every other listing leaves out.
#[derive(Debug, Deserialize)]
struct PageQuery {
    limit: Option<usize>,
    #[serde(default)]
    offset: usize,
    label_id: Option<String>,
    #[serde(default)]
    starred: bool,
    #[serde(default)]
    archived: bool,
}

impl PageQuery {
    fn parts(self) -> Result<(MeetingFilter, usize, usize), ApiError> {
        let limit = self.limit.unwrap_or(DEFAULT_LIMIT);
        if !(1..=MAX_LIMIT).contains(&limit) {
            return Err(ApiError::BadRequest(format!(
                "limit must be between 1 and {MAX_LIMIT}"
            )));
        }
        let filter = MeetingFilter {
            label_id: self.label_id,
            starred: self.starred,
            archived: self.archived,
        };
        Ok((filter, limit, self.offset))
    }
}

/// An empty `q` falls through to the chronological list. Paging comes from a second `Query<PageQuery>`, since serde cannot flatten typed fields out of a query string.
#[derive(Debug, Deserialize)]
struct SearchQuery {
    q: String,
}

async fn search(
    State(s): State<AppState>,
    Query(qs): Query<SearchQuery>,
    Query(page): Query<PageQuery>,
) -> Result<Json<MeetingPage>, ApiError> {
    let (filter, limit, offset) = page.parts()?;
    let repo = s.meeting_repo.clone();
    let page = tokio::task::spawn_blocking(move || repo.search_page(&qs.q, &filter, limit, offset))
        .await
        .map_err(|e| ApiError::Internal(anyhow::Error::new(e)))?
        .map_err(ApiError::from)?;
    Ok(Json(page))
}

async fn list(
    State(s): State<AppState>,
    Query(qs): Query<PageQuery>,
) -> Result<Json<MeetingPage>, ApiError> {
    let (filter, limit, offset) = qs.parts()?;
    let repo = s.meeting_repo.clone();
    let page = tokio::task::spawn_blocking(move || repo.list_page(&filter, limit, offset))
        .await
        .map_err(|e| ApiError::Internal(anyhow::Error::new(e)))?
        .map_err(ApiError::from)?;
    Ok(Json(page))
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
    let (opt, me) = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let m = repo.get(&id)?;
        let me = match m.as_ref().and_then(|m| m.source.as_deref()) {
            Some(src) => repo.most_common_source_owner(src)?,
            None => None,
        };
        Ok((m, me))
    })
    .await
    .map_err(|e| ApiError::Internal(anyhow::Error::new(e)))?
    .map_err(ApiError::from)?;
    let mut m = opt.ok_or(ApiError::NotFound)?;
    mark_owner_is_user(&mut m, me.as_deref());
    Ok(Json(m))
}

/// Adds `owner.is_user` to an imported meeting's `source_json`, decided like the import preview's
/// "captured by someone else": the owner of most imports stands in for the user.
fn mark_owner_is_user(m: &mut Meeting, me: Option<&str>) {
    let Some(owner) = m
        .source_json
        .as_mut()
        .and_then(|j| j.get_mut("owner"))
        .and_then(|o| o.as_object_mut())
    else {
        return;
    };
    let is_user = me.is_some() && owner.get("email").and_then(|e| e.as_str()) == me;
    owner.insert("is_user".into(), is_user.into());
}

async fn patch_one(
    State(s): State<AppState>,
    Path(id): Path<String>,
    body: Option<Json<PatchBody>>,
) -> Result<Json<Meeting>, ApiError> {
    let mut body = body.map(|Json(b)| b).unwrap_or_default();
    if body.archived == Some(true)
        && s.meetings
            .active_recording()
            .await
            .is_some_and(|a| a.to_string() == id)
    {
        return Err(ApiError::Conflict(
            "the meeting being recorded cannot be archived".into(),
        ));
    }
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
        archived: body.archived,
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

#[derive(Debug, Deserialize)]
struct ArchiveBody {
    /// Unix ms. Meetings that started before it are archived.
    before: i64,
    #[serde(default)]
    preview: bool,
}

/// `POST /api/meetings/archive`: archive every meeting that started before `before`, or with `preview` only count them.
/// One `UPDATE` rather than a PATCH per meeting, since an import can leave over a thousand to archive.
async fn archive_before(
    State(s): State<AppState>,
    Json(body): Json<ArchiveBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let recording = s.meetings.active_recording().await.map(|id| id.to_string());
    let repo = s.meeting_repo.clone();
    let count = tokio::task::spawn_blocking(move || {
        repo.archive_before(body.before, recording.as_deref(), body.preview)
    })
    .await
    .map_err(|e| ApiError::Internal(anyhow::Error::new(e)))?
    .map_err(ApiError::from)?;
    Ok(Json(serde_json::json!({ "count": count })))
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
    let removed = tokio::task::spawn_blocking(move || -> anyhow::Result<bool> {
        let m = if q.delete_file { repo.get(&id)? } else { None };
        let removed = repo.delete(&id)?;
        // Unlink only after the row is gone. A missing file is fine since the exporter writes lazily.
        if removed {
            if let Some(m) = m {
                if let Err(e) = exporter.remove(&exporter_view(&m)) {
                    tracing::warn!(?e, "delete: could not remove markdown file");
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

#[cfg(test)]
mod tests {
    use crate::AppState;
    use serde_json::{json, Value};
    use std::sync::Arc;
    use yogurt_db::NewMeeting;

    struct Harness {
        base: String,
        token: String,
        state: AppState,
        http: reqwest::Client,
        _tmp: tempfile::TempDir,
    }

    impl Harness {
        async fn new() -> Self {
            let tmp = tempfile::tempdir().unwrap();
            let storage =
                Arc::new(crate::storage::Storage::init_at(&tmp.path().join("db.sqlite")).unwrap());
            let session = Arc::new(
                crate::session::load_or_create(&tmp.path().join("session-token")).unwrap(),
            );
            let state = AppState::in_memory(
                crate::Mode::Release,
                storage,
                session,
                7878,
                tmp.path().join("notes"),
            )
            .unwrap();
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let base = format!("http://{}", listener.local_addr().unwrap());
            let app = crate::routes::router(state.clone());
            tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
            Self {
                base,
                token: state.session.as_str().to_string(),
                state,
                http: reqwest::Client::new(),
                _tmp: tmp,
            }
        }

        async fn call(&self, m: reqwest::Method, p: &str, body: Option<Value>) -> (u16, Value) {
            let mut req = self
                .http
                .request(m, format!("{}{p}", self.base))
                .bearer_auth(&self.token);
            if let Some(b) = body {
                req = req.json(&b);
            }
            let resp = req.send().await.unwrap();
            let code = resp.status().as_u16();
            (code, resp.json().await.unwrap_or(Value::Null))
        }

        /// A meeting in the registry and the DB under one id, `recording` giving it a live task.
        async fn meeting(&self, title: &str, started_at: i64, recording: bool) -> String {
            let live = self.state.meetings.create().await;
            let id = live.id.to_string();
            self.state
                .meeting_repo
                .create(NewMeeting {
                    title: title.into(),
                    started_at_unix_ms: Some(started_at),
                    id: Some(id.clone()),
                })
                .unwrap();
            if recording {
                *live.task.lock().await = Some(tokio::spawn(std::future::pending()));
            }
            id
        }

        async fn patch(&self, id: &str, archived: bool) -> (u16, Value) {
            let p = format!("/api/meetings/{id}");
            self.call(
                reqwest::Method::PATCH,
                &p,
                Some(json!({ "archived": archived })),
            )
            .await
        }

        async fn titles(&self, qs: &str) -> Vec<String> {
            let (_, page) = self
                .call(reqwest::Method::GET, &format!("/api/meetings{qs}"), None)
                .await;
            page["meetings"]
                .as_array()
                .unwrap()
                .iter()
                .map(|m| m["title"].as_str().unwrap().to_string())
                .collect()
        }
    }

    #[tokio::test]
    async fn the_recording_cannot_be_archived_by_patch_or_by_date() {
        let h = Harness::new().await;
        let rec = h.meeting("Recording", 1_000, true).await;
        let old = h.meeting("Old", 2_000, false).await;
        h.meeting("New", 9_000, false).await;

        let (code, body) = h.patch(&rec, true).await;
        assert_eq!(code, 409, "{body}");
        assert!(h
            .state
            .meeting_repo
            .get(&rec)
            .unwrap()
            .unwrap()
            .archived_at
            .is_none());

        let archive = |preview: bool| {
            h.call(
                reqwest::Method::POST,
                "/api/meetings/archive",
                Some(json!({ "before": 5_000, "preview": preview })),
            )
        };
        assert_eq!(archive(true).await, (200, json!({ "count": 1 })));
        assert_eq!(h.titles("?archived=true").await, Vec::<String>::new());
        assert_eq!(archive(false).await, (200, json!({ "count": 1 })));
        assert_eq!(h.titles("?archived=true").await, ["Old"]);
        assert_eq!(h.titles("").await, ["New", "Recording"]);
        assert_eq!(h.titles("/search?q=old&archived=true").await, ["Old"]);
        assert_eq!(h.titles("/search?q=old").await, Vec::<String>::new());

        let (code, m) = h.patch(&old, false).await;
        assert_eq!(code, 200);
        assert!(
            m.get("archived_at").is_none(),
            "unarchived meetings omit the field"
        );
        let (code, m) = h.patch(&old, true).await;
        assert_eq!(code, 200);
        assert!(m["archived_at"].is_string());
    }
}
