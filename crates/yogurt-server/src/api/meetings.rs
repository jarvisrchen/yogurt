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
//! is always in sync with the SQLite row. DELETE intentionally does NOT
//! remove the markdown file (D-10 / PRD §5.7) — the file is the user's
//! grep-able source of truth even after they purge the library entry.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;

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
}

/// Centralized error → response mapping. The error message bubbles through
/// the response body so REST consumers can surface the underlying problem;
/// the wire shape matches the existing `routes::*` handlers'
/// `{"error": "<msg>"}` envelope.
#[derive(Debug)]
enum ApiError {
    NotFound,
    BadRequest(String),
    Internal(anyhow::Error),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (code, msg) = match self {
            ApiError::NotFound => (StatusCode::NOT_FOUND, "meeting not found".to_string()),
            ApiError::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            ApiError::Internal(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}")),
        };
        (code, Json(json!({ "error": msg }))).into_response()
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        // Map the specific "not found" string the repo bails with into 404
        // so PATCH / DELETE behave as REST clients expect.
        let s = e.to_string();
        if s.contains("not found") {
            ApiError::NotFound
        } else if s.contains("empty") {
            ApiError::BadRequest(s)
        } else {
            ApiError::Internal(e)
        }
    }
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
    let body = body.map(|Json(b)| b).unwrap_or_default();
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

async fn delete_one(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let repo = s.meeting_repo.clone();
    // Intentional: do NOT touch the markdown file on disk. PRD §5.7 + D-10
    // — the markdown file in ~/.yogurt/notes/ is the user's grep-able source
    // of truth and survives Library deletion. SQLite cascade purges the
    // chat rows.
    let removed = tokio::task::spawn_blocking(move || repo.delete(&id))
        .await
        .map_err(|e| ApiError::Internal(anyhow::Error::new(e)))?
        .map_err(ApiError::from)?;
    if removed {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}
