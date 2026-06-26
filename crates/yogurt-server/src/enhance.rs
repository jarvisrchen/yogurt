//! Phase 4 hero augmented-notes endpoint (CONTEXT D-22 / D-23).
//!
//! `POST /api/meetings/{id}/enhance` accepts the user's raw notes plus the
//! transcript-so-far in the request body, renders the bundled `enhance.md`
//! prompt, calls the LLM (real `OpenAiCompatClient` if env vars present,
//! else the deterministic `MockLlm`), runs `yogurt_notes::merge_notes` over
//! the result, persists `enriched_md` + `enriched_doc_json` to SQLite and
//! to a per-meeting markdown file under `~/.yogurt/notes/`, and emits
//! three `enhance_progress` WebSocket events (`sending` → `streaming` →
//! `done`) on the meeting's events broadcast.
//!
//! ## Why the body carries `notes_md` + `transcript_json`
//!
//! The in-memory `Meeting` struct (`meetings.rs`) holds only the live audio
//! and transcript broadcasts plus capture handles. It does NOT store the
//! user's raw notes or a serialized transcript snapshot. Adding those
//! fields would require persisting transcript-events as they arrive (out
//! of scope for Plan 04-03) plus a contract for client-side notes-edit
//! WS frames (Phase 4 deferred). The pragmatic adaptation is to let the
//! browser send both as the enhance request body; Phase 5+ replaces this
//! with server-side accumulation. The endpoint itself is the contract that
//! matters: request body, persistence, WS events, and response shape are
//! all on the public surface from day one.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

use crate::llm_mock::MockLlm;
use crate::llm_openai::OpenAiCompatClient;
use crate::markdown_exporter::Meeting as ExpMeeting;
use crate::AppState;
use yogurt_notes::merge_notes;
use yogurt_prompts::EnhanceCtx;

#[derive(Debug, Deserialize)]
pub struct EnhanceRequest {
    /// User's raw markdown notes — preserved verbatim across the merge.
    pub notes_md: String,
    /// JSON-serialized transcript segments (the same shape `yogurt_notes`
    /// already parses: `[{ts_ms, channel, text}]`). Empty array if the
    /// meeting has no transcript yet (the LLM still gets the notes).
    pub transcript_json: String,
    /// Optional meeting title — drives the markdown file slug. Defaults
    /// to `"untitled"` if absent or blank.
    #[serde(default)]
    pub title: Option<String>,
    /// Optional meeting start time (unix ms). Defaults to the meeting's
    /// in-memory `created_at_ms` so the filename always reflects when
    /// recording started.
    #[serde(default)]
    pub started_at_unix_ms: Option<i64>,
    /// Optional meeting end time (unix ms). Persisted to the YAML
    /// front-matter; `null` if not provided.
    #[serde(default)]
    pub ended_at_unix_ms: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct EnhanceResponse {
    /// Wire-format enriched markdown — `<span data-ai-grey…>` /
    /// `<span data-transcript-link…>` spans embedded inline. The browser
    /// loads this into the YogurtEditor via `setContent(html, false)`.
    pub enriched_md: String,
    /// Path on disk where the per-meeting markdown file was written.
    /// Mostly useful for the integration test; the browser ignores it.
    pub notes_file: String,
}

/// `POST /api/meetings/{id}/enhance`.
///
/// Auth is enforced one layer up by `routes::require_session_token`.
/// On success: HTTP 200 + `EnhanceResponse`. On failure: a tuple of
/// `(StatusCode, plaintext-message)` that axum converts to a JSON-friendly
/// error body.
pub async fn enhance(
    State(state): State<AppState>,
    Path(meeting_id): Path<Uuid>,
    Json(req): Json<EnhanceRequest>,
) -> Result<Json<EnhanceResponse>, (StatusCode, String)> {
    // 1) Look up the meeting (404 if unknown).
    let meeting = state.meetings.get(&meeting_id).await.ok_or((
        StatusCode::NOT_FOUND,
        format!("meeting {meeting_id} not found"),
    ))?;

    // 2) Render the user prompt.
    let user_prompt = state
        .prompts
        .render_enhance(&EnhanceCtx {
            notes: &req.notes_md,
            transcript: &req.transcript_json,
        })
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("prompt render: {e}"),
            )
        })?;

    // 3) Emit `enhance_progress` — sending. `send` returns Err only when
    // there are no subscribers; that's fine, the user just doesn't have
    // a WS open. Don't fail the request on it.
    let _ = meeting
        .events_tx
        .send(serde_json::json!({"type": "enhance_progress", "phase": "sending"}));

    // 4) Call the LLM. OpenAiCompat from env when configured, else MockLlm.
    let llm_output = match OpenAiCompatClient::from_env() {
        Some(client) => client
            .complete("", &user_prompt)
            .await
            .map_err(|e| (StatusCode::BAD_GATEWAY, format!("llm complete failed: {e}")))?,
        None => MockLlm
            .complete("", &user_prompt)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("mock llm: {e}")))?,
    };

    // 5) Emit `enhance_progress` — streaming. Phase 4 reports the final
    // character count once; Phase 5 will stream per-chunk.
    let _ = meeting.events_tx.send(serde_json::json!({
        "type": "enhance_progress",
        "phase": "streaming",
        "chars": llm_output.len(),
    }));

    // 6) Server-side structural diff (Plan 04-02). Returns a MergedDoc
    // tagging each block User vs. AiGrey + timestamp.
    let merged = merge_notes(&req.notes_md, &llm_output, &req.transcript_json).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("merge_notes: {e}"),
        )
    })?;
    let rendered_md = yogurt_notes::render::to_markdown(&merged);
    // BL-2 (XSS hardening): run the rendered markdown through ammonia with a
    // tight allowlist so anything the LLM hallucinated (script/img/iframe/
    // on* handlers/javascript: URLs) is stripped BEFORE we persist to SQLite
    // or write to disk. The wire-format spans (<span data-ai-grey>, <span
    // data-transcript-link>) survive. Layered with render::wrap_ai's
    // html-escape of inner text, this neutralizes the stored-XSS path
    // documented in 04-REVIEW.md BL-2.
    let enriched_md = crate::sanitize::sanitize_enriched_md(&rendered_md);
    let enriched_doc_json = serde_json::to_string(&merged).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("serialize merged doc: {e}"),
        )
    })?;

    // 7a/7b) Persist to SQLite + per-meeting markdown file inside a single
    // spawn_blocking. BL-1: rusqlite is synchronous (std::sync::Mutex +
    // libsqlite3 calls) and the markdown exporter does std::fs::write +
    // std::fs::rename (and now fsync, per HI-5). All of those block the
    // executor thread for the duration of the disk write. Putting them on
    // a blocking pool keeps the tokio worker free to drive other meetings'
    // WS frames + /api/health probes while a slow disk pauses one enhance.
    let title = req.title.as_deref().unwrap_or("untitled").to_string();
    let started_at_unix_ms = req
        .started_at_unix_ms
        .unwrap_or(meeting.created_at_ms as i64);
    let ended_at_unix_ms = req.ended_at_unix_ms;
    let meeting_id_str = meeting_id.to_string();

    let storage = state.storage.clone();
    let exporter = state.markdown_exporter.clone();
    let notes_md_for_blocking = req.notes_md.clone();
    let transcript_json_for_blocking = req.transcript_json.clone();
    let enriched_md_for_blocking = enriched_md.clone();
    let enriched_doc_json_for_blocking = enriched_doc_json.clone();
    let title_for_blocking = title.clone();
    let meeting_id_for_blocking = meeting_id_str.clone();

    let notes_path: PathBuf =
        tokio::task::spawn_blocking(move || -> Result<PathBuf, (StatusCode, String)> {
            // SQLite UPSERT inside its own lock scope so the lock is released
            // BEFORE the filesystem write begins. Other writers can proceed
            // while this task is in std::fs::rename.
            {
                let writer = storage.writer();
                let conn = writer
                    .lock()
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("db lock: {e}")))?;
                conn.execute(
                    r#"
                    INSERT INTO meetings
                        (id, title, started_at, ended_at, notes_md, transcript_json,
                         enriched_md, enriched_doc_json)
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                    ON CONFLICT(id) DO UPDATE SET
                        title             = excluded.title,
                        ended_at          = COALESCE(excluded.ended_at, meetings.ended_at),
                        notes_md          = excluded.notes_md,
                        transcript_json   = excluded.transcript_json,
                        enriched_md       = excluded.enriched_md,
                        enriched_doc_json = excluded.enriched_doc_json
                    "#,
                    rusqlite::params![
                        meeting_id_for_blocking,
                        title_for_blocking,
                        started_at_unix_ms,
                        ended_at_unix_ms,
                        notes_md_for_blocking,
                        transcript_json_for_blocking,
                        enriched_md_for_blocking,
                        enriched_doc_json_for_blocking,
                    ],
                )
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("db upsert: {e}")))?;
                // lock released on scope exit
            }

            // Filesystem write (atomic tmp+rename+fsync) — see markdown_exporter.
            exporter
                .write(&ExpMeeting {
                    id: &meeting_id_for_blocking,
                    title: &title_for_blocking,
                    started_at_unix_ms,
                    ended_at_unix_ms,
                    body_md: &enriched_md_for_blocking,
                })
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("markdown exporter: {e}"),
                    )
                })
        })
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("spawn_blocking join: {e}"),
            )
        })??;

    // 8) Emit `enhance_progress` — done.
    let _ = meeting
        .events_tx
        .send(serde_json::json!({"type": "enhance_progress", "phase": "done"}));

    Ok(Json(EnhanceResponse {
        enriched_md,
        notes_file: notes_path.to_string_lossy().into_owned(),
    }))
}
