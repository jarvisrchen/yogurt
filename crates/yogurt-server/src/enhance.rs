//! `POST /api/meetings/{id}/enhance`: merges the user's notes with the LLM's output and streams `enhance_progress` frames.
//!
//! The request body carries `notes_md` and `transcript_json` because the in-memory `Meeting` stores neither.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
use uuid::Uuid;

use crate::markdown_exporter::Meeting as ExpMeeting;
use crate::AppState;
use yogurt_llm::{ChatMessage, ChatRequest};
use yogurt_notes::enhance::EnhanceError;
use yogurt_prompts::{split_template_line, EnhanceCtx, Template};

#[derive(Debug, Deserialize)]
pub struct EnhanceRequest {
    pub notes_md: String,
    /// `[{ts_ms, channel, text}]`, the shape `yogurt_notes` parses.
    pub transcript_json: String,
    /// Drives the markdown file slug. Defaults to `"untitled"`.
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub started_at_unix_ms: Option<i64>,
    #[serde(default)]
    pub ended_at_unix_ms: Option<i64>,
    /// Note format to shape the summary into, an id from
    /// `yogurt_prompts::TEMPLATE_IDS`. Absent, empty, or `"auto"` lets the
    /// model pick the best fit itself (the default flow); anything else
    /// unknown is a 400.
    #[serde(default)]
    pub template: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct EnhanceResponse {
    /// Markdown with inline `<span data-ai-grey>` / `<span data-transcript-link>` spans.
    pub enriched_md: String,
    pub notes_file: String,
    /// The LLM/merge/persist pipeline was skipped, so `enriched_md` and `notes_file` are empty.
    #[serde(default)]
    pub too_short: bool,
    /// `None` when `too_short`.
    #[serde(default)]
    pub llm_model: Option<String>,
    /// The requested format, or the one the model picked (`general` if it named nothing usable).
    /// `None` when `too_short`.
    #[serde(default)]
    pub template: Option<String>,
}

pub async fn list_templates(
    State(state): State<AppState>,
) -> Result<Json<Vec<Template>>, (StatusCode, String)> {
    state
        .prompts
        .templates()
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("templates: {e}")))
}

pub async fn enhance(
    State(state): State<AppState>,
    Path(meeting_id): Path<Uuid>,
    Json(req): Json<EnhanceRequest>,
) -> Result<Json<EnhanceResponse>, (StatusCode, String)> {
    // The in-memory registry is wiped on restart, so Re-enhance falls back to
    // re-hydrating from SQLite.
    let meeting = match state.meetings.get(&meeting_id).await {
        Some(m) => m,
        None => {
            let id_str = meeting_id.to_string();
            let reader = state.storage.read();
            let exists = {
                let conn = reader
                    .lock()
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("db lock: {e}")))?;
                conn.query_row(
                    "SELECT 1 FROM meetings WHERE id = ?1",
                    rusqlite::params![id_str],
                    |_| Ok(()),
                )
                .is_ok()
            };
            if !exists {
                return Err((
                    StatusCode::NOT_FOUND,
                    format!("meeting {meeting_id} not found"),
                ));
            }
            // No audio/transcript broadcasts will fire, but events_tx carries enhance_progress.
            state.meetings.hydrate(meeting_id).await
        }
    };

    let user_notes = req.notes_md.clone();

    // `None` means "let the model choose". An unknown name is a 400, not a silent fallback.
    let forced_template = yogurt_prompts::template_choice(req.template.as_deref())
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    // The server-side STT persistence task (meetings.rs) is the source of truth.
    // Use the stored row whenever the request's copy is empty.
    let transcript_json = {
        let req_empty = matches!(req.transcript_json.trim(), "" | "[]");
        if req_empty {
            let id = meeting_id.to_string();
            let repo = state.meeting_repo.clone();
            tokio::task::spawn_blocking(move || repo.get(&id))
                .await
                .ok()
                .and_then(|r| r.ok())
                .flatten()
                .map(|m| m.transcript_json)
                .filter(|t| !matches!(t.trim(), "" | "[]"))
                .unwrap_or_else(|| req.transcript_json.clone())
        } else {
            req.transcript_json.clone()
        }
    };

    if yogurt_notes::enhance::too_short(&user_notes, &transcript_json) {
        // Drop the stub row from an accidental start/stop, but never a meeting
        // that already has an enriched body.
        let repo = state.meeting_repo.clone();
        let exporter = state.markdown_exporter.clone();
        let id = meeting_id.to_string();
        let _ = tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            let Some(m) = repo.get(&id)? else {
                return Ok(());
            };
            if m.enriched_md
                .as_deref()
                .is_some_and(|e| !e.trim().is_empty())
            {
                return Ok(());
            }
            if repo.delete(&id)? {
                let view = ExpMeeting {
                    id: &m.id,
                    title: &m.title,
                    started_at_unix_ms: m.started_at,
                    ended_at_unix_ms: m.ended_at,
                    body_md: &m.notes_md,
                };
                if let Err(e) = exporter.remove(&view) {
                    tracing::warn!(?e, "too-short: could not remove markdown file");
                }
            }
            Ok(())
        })
        .await;
        tracing::info!(
            event = "enhance_skipped_too_short",
            meeting_id = %meeting_id,
            "enhance skipped: meeting too short"
        );
        return Ok(Json(EnhanceResponse {
            enriched_md: String::new(),
            notes_file: String::new(),
            too_short: true,
            llm_model: None,
            template: None,
        }));
    }

    let user_prompt = state
        .prompts
        .render_enhance(&EnhanceCtx {
            notes: &user_notes,
            transcript: &transcript_json,
            template: forced_template,
        })
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("prompt render: {e}"),
            )
        })?;

    // `send` errs only when no WS is subscribed, which is fine.
    let _ = meeting
        .events_tx
        .send(serde_json::json!({"type": "enhance_progress", "phase": "sending"}));

    // A configured provider whose key can't be read is a hard 502, never a silent mock fallback.
    //
    // The timeout bounds opening the stream and then each chunk (idle timeout),
    // so a long healthy stream is not cut off. It comes from the client because
    // `stream()` resolves at response headers for HTTP but after full generation
    // for a CLI provider, so no single constant fits both.
    //
    // No system message: `enhance.md` is rendered as the whole user payload.
    let llm =
        match crate::llm_openai::resolve(&state, "enhance", Some(meeting_id.to_string())).await {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(
                    event = "enhance_llm_resolution_failed",
                    meeting_id = %meeting_id,
                    error = %e,
                    "enhance failed to resolve LLM provider"
                );
                let _ = meeting.events_tx.send(serde_json::json!({
                    "type": "enhance_progress",
                    "phase": "error",
                    "message": format!("Enhance failed: {e}")
                }));
                return Err((StatusCode::BAD_GATEWAY, e.to_string()));
            }
        };
    let chat_req = ChatRequest {
        messages: vec![ChatMessage::user(user_prompt.clone())],
        stream: true,
    };
    let llm_model = llm.model_name();
    let llm_timeout = llm.response_timeout();
    let llm_started = Instant::now();

    let llm_failed = |error: &anyhow::Error| -> (StatusCode, String) {
        let msg = format!("llm stream failed: {error:#}");
        tracing::error!(
            event = "enhance_llm_request_failed",
            meeting_id = %meeting_id,
            model = %llm_model,
            elapsed_ms = llm_started.elapsed().as_millis(),
            error = %format_args!("{error:#}"),
            "enhance LLM request failed"
        );
        let _ = meeting.events_tx.send(serde_json::json!({
            "type": "enhance_progress",
            "phase": "error",
            "message": format!("Enhance failed: {msg}")
        }));
        (StatusCode::BAD_GATEWAY, msg)
    };
    let llm_timed_out = || -> (StatusCode, String) {
        tracing::error!(
            event = "enhance_llm_request_timed_out",
            meeting_id = %meeting_id,
            model = %llm_model,
            elapsed_ms = llm_started.elapsed().as_millis(),
            timeout_secs = llm_timeout.as_secs(),
            "enhance LLM request timed out"
        );
        // A timeout is far more often a slow model than a provider outage, so name the model.
        let _ = meeting.events_tx.send(serde_json::json!({
            "type": "enhance_progress",
            "phase": "error",
            "message": format!(
                "{llm_model} did not respond within {}s - try a faster model in Settings",
                llm_timeout.as_secs()
            )
        }));
        (
            StatusCode::GATEWAY_TIMEOUT,
            format!(
                "llm call to {llm_model} exceeded {}s timeout",
                llm_timeout.as_secs()
            ),
        )
    };

    let mut stream = match tokio::time::timeout(llm_timeout, llm.stream(chat_req)).await {
        Ok(Ok(s)) => s,
        Ok(Err(error)) => return Err(llm_failed(&error)),
        Err(_elapsed) => return Err(llm_timed_out()),
    };

    // Roughly 12 Hz keeps the browser-side TipTap `setContent` calls from thrashing the editor.
    const STREAM_SNAPSHOT_INTERVAL: Duration = Duration::from_millis(80);

    let mut llm_output = String::new();
    let mut last_emit: Option<Instant> = None;
    let mut last_emitted_len: usize = 0;

    loop {
        let chunk = match tokio::time::timeout(llm_timeout, stream.next()).await {
            Ok(Some(Ok(c))) => c,
            Ok(Some(Err(error))) => return Err(llm_failed(&error)),
            // Stream ended without a `done` chunk: keep what was accumulated.
            Ok(None) => break,
            Err(_elapsed) => return Err(llm_timed_out()),
        };

        if !chunk.delta.is_empty() {
            llm_output.push_str(&chunk.delta);
            let now = Instant::now();
            let should_emit = match last_emit {
                None => true,
                Some(t) => now.duration_since(t) >= STREAM_SNAPSHOT_INTERVAL,
            };
            if should_emit {
                // The browser previews the document, not the `template: <id>` marker line.
                let text = split_template_line(&llm_output).1;
                let _ = meeting.events_tx.send(serde_json::json!({
                    "type": "enhance_progress",
                    "phase": "streaming",
                    "chars": text.len(),
                    "text": text,
                }));
                last_emit = Some(now);
                last_emitted_len = llm_output.len();
            }
        }

        if chunk.done {
            break;
        }
    }

    // Throttling may have dropped the tail. The broadcast channel has no replay,
    // so the last frame must match the full text.
    if llm_output.len() != last_emitted_len {
        let text = split_template_line(&llm_output).1;
        let _ = meeting.events_tx.send(serde_json::json!({
            "type": "enhance_progress",
            "phase": "streaming",
            "chars": text.len(),
            "text": text,
        }));
    }

    let enhanced = match yogurt_notes::enhance::finish(
        &user_notes,
        &transcript_json,
        &llm_output,
        forced_template,
    ) {
        Ok(e) => e,
        Err(EnhanceError::EmptyReply) => {
            tracing::error!(
                event = "enhance_llm_empty_reply",
                meeting_id = %meeting_id,
                model = %llm_model,
                "enhance LLM returned no text"
            );
            let _ = meeting.events_tx.send(serde_json::json!({
                "type": "enhance_progress",
                "phase": "error",
                "message": format!(
                    "{llm_model} returned no text - try Re-enhance or a different model in Settings"
                )
            }));
            return Err((
                StatusCode::BAD_GATEWAY,
                format!("llm {llm_model} returned an empty reply"),
            ));
        }
        Err(EnhanceError::Merge(e)) => {
            tracing::error!(
                event = "enhance_merge_failed",
                meeting_id = %meeting_id,
                model = %llm_model,
                error = %e,
                "enhance failed to merge notes"
            );
            let _ = meeting.events_tx.send(serde_json::json!({
                "type": "enhance_progress",
                "phase": "error",
                "message": format!("Merge failed: {e}")
            }));
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("merge_notes: {e}"),
            ));
        }
    };
    if let Some(picked) = &enhanced.unrecognized_pick {
        tracing::warn!(
            event = "enhance_template_unrecognized",
            meeting_id = %meeting_id,
            model = %llm_model,
            picked = ?picked,
            "model named no known template; using general"
        );
    }
    let template = enhanced.template;
    let merged = enhanced.doc;
    let enriched_md = enhanced.enriched_md;
    let enriched_doc_json = serde_json::to_string(&merged).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("serialize merged doc: {e}"),
        )
    })?;

    // rusqlite and the markdown exporter's fs writes are synchronous, so run them
    // on the blocking pool to keep the tokio worker free.
    let title = req.title.as_deref().unwrap_or("untitled").to_string();
    let started_at_unix_ms = req
        .started_at_unix_ms
        .unwrap_or(meeting.created_at_ms as i64);
    let ended_at_unix_ms = req.ended_at_unix_ms;
    let meeting_id_str = meeting_id.to_string();

    let storage = state.storage.clone();
    let exporter = state.markdown_exporter.clone();
    let notes_md_for_blocking = user_notes.clone();
    let transcript_json_for_blocking = transcript_json.clone();
    let enriched_md_for_blocking = enriched_md.clone();
    let enriched_doc_json_for_blocking = enriched_doc_json.clone();
    let title_for_blocking = title.clone();
    let meeting_id_for_blocking = meeting_id_str.clone();

    let notes_path =
        tokio::task::spawn_blocking(move || -> Result<PathBuf, (StatusCode, String)> {
            // Release the DB lock before the filesystem write begins.
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
            }

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
        .await;

    let notes_path: PathBuf = match notes_path {
        Ok(Ok(p)) => p,
        Ok(Err((code, msg))) => {
            tracing::error!(
                event = "enhance_persistence_failed",
                meeting_id = %meeting_id,
                error = %msg,
                "enhance failed to persist output"
            );
            let _ = meeting.events_tx.send(serde_json::json!({
                "type": "enhance_progress",
                "phase": "error",
                "message": format!("Persistence failed: {msg}")
            }));
            return Err((code, msg));
        }
        Err(join_err) => {
            tracing::error!(
                event = "enhance_persistence_task_failed",
                meeting_id = %meeting_id,
                error = %join_err,
                "enhance persistence task failed"
            );
            let _ = meeting.events_tx.send(serde_json::json!({
                "type": "enhance_progress",
                "phase": "error",
                "message": "Persistence task failed"
            }));
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("spawn_blocking join: {join_err}"),
            ));
        }
    };

    // `GET /api/meetings` reads from `yogurt_db::MeetingRepo`, so mirror the
    // post-enhance state there. Creates the row if the Library never saw it.
    {
        let id = meeting_id_str.clone();
        let title = title.clone();
        let notes_md = user_notes.clone();
        let transcript_json = transcript_json.clone();
        let enriched_md_for_repo = enriched_md.clone();
        let llm_model = llm.model_name();
        let started_at_fallback = started_at_unix_ms;
        // Only touch the repo's timestamps when the request carried them. An absent
        // field must not map to `Some(None)`, which would clear the ended_at /stop wrote.
        let started_at_patch = req.started_at_unix_ms;
        let ended_at_patch = req.ended_at_unix_ms.map(Some);
        let state2 = state.clone();
        let mirror_result = tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            if state2.meeting_repo.get(&id)?.is_none() {
                state2.meeting_repo.create(yogurt_db::NewMeeting {
                    title: title.clone(),
                    started_at_unix_ms: Some(started_at_fallback),
                    id: Some(id.clone()),
                })?;
            }
            state2.meeting_repo.patch(
                &id,
                yogurt_db::MeetingPatch {
                    title: Some(title),
                    started_at: started_at_patch,
                    notes_md: Some(notes_md),
                    transcript_json: Some(transcript_json),
                    enriched_md: Some(Some(enriched_md_for_repo)),
                    ended_at: ended_at_patch,
                    starred: None,
                    archived: None,
                    stt_engine: None,
                    llm_model: Some(llm_model),
                    template: Some(template.to_string()),
                    label_ids: None,
                },
            )?;
            Ok(())
        })
        .await;
        match mirror_result {
            Ok(Ok(())) => state
                .cloud_push
                .request(chrono::Utc::now().timestamp_millis()),
            Ok(Err(e)) => tracing::warn!(
                event = "enhance_library_mirror_failed",
                meeting_id = %meeting_id,
                error = %e,
                "enhance output persisted but Library mirror failed"
            ),
            Err(e) => tracing::warn!(
                event = "enhance_library_mirror_task_failed",
                meeting_id = %meeting_id,
                error = %e,
                "enhance output persisted but Library mirror task failed"
            ),
        }
    }

    crate::recap::close(&meeting);
    let _ = meeting
        .events_tx
        .send(serde_json::json!({"type": "enhance_progress", "phase": "done"}));

    tracing::info!(
        event = "enhance_complete",
        meeting_id = %meeting_id,
        model = %llm_model,
        template,
        forced = forced_template.is_some(),
        elapsed_ms = llm_started.elapsed().as_millis(),
        "enhance complete"
    );

    Ok(Json(EnhanceResponse {
        enriched_md,
        notes_file: notes_path.to_string_lossy().into_owned(),
        too_short: false,
        llm_model: Some(llm_model),
        template: Some(template.to_string()),
    }))
}

pub(crate) fn strip_model_reasoning(s: &str) -> String {
    yogurt_llm::strip_thinking(s)
}

#[cfg(test)]
mod tests {
    use super::strip_model_reasoning;

    #[test]
    fn strips_model_reasoning_from_content() {
        let output =
            "<think>\nI should analyze the transcript.\n</think>\n\n## Decisions\n- Ship Monday";
        assert_eq!(strip_model_reasoning(output), "## Decisions\n- Ship Monday");
    }
}
