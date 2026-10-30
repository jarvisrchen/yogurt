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
use yogurt_notes::{merge_notes, TranscriptSegment};
use yogurt_prompts::{split_template_line, EnhanceCtx, Template, TEMPLATE_IDS};

/// A meeting with no typed notes and fewer transcript words than this is an
/// accidental start/stop. Deliberately low: a real single sentence
/// ("today is my last day, thanks everyone") must still enhance.
const TOO_SHORT_TRANSCRIPT_WORDS: usize = 6;

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
    let forced_template: Option<&'static str> = match req.template.as_deref().map(str::trim) {
        None | Some("") | Some("auto") => None,
        Some(id) => match TEMPLATE_IDS.iter().copied().find(|t| *t == id) {
            Some(known) => Some(known),
            None => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    format!(
                        "unknown template {id:?}; expected one of: {}",
                        TEMPLATE_IDS.join(", ")
                    ),
                ))
            }
        },
    };

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

    let transcript_word_count: usize =
        serde_json::from_str::<Vec<TranscriptSegment>>(&transcript_json)
            .unwrap_or_default()
            .iter()
            .map(|seg| seg.text.split_whitespace().count())
            .sum();
    if user_notes.trim().is_empty() && transcript_word_count < TOO_SHORT_TRANSCRIPT_WORDS {
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

    // Some models (MiniMax-Text-01) emit CP1252-mangled punctuation and echo the
    // prompt's section scaffolding, so scrub both before storing.
    // A forced format wins over the model's `template:` line. In auto mode an
    // unknown or missing id falls back to `general` since the document is still good.
    let llm_output = strip_model_reasoning(&fix_model_mojibake(&llm_output));
    let (picked, body) = split_template_line(&llm_output);
    let template: &str = match forced_template {
        Some(id) => id,
        None => match picked.map(|p| p.to_ascii_lowercase()) {
            Some(p) if TEMPLATE_IDS.contains(&p.as_str()) => {
                TEMPLATE_IDS.iter().copied().find(|t| *t == p).unwrap()
            }
            other => {
                tracing::warn!(
                    event = "enhance_template_unrecognized",
                    meeting_id = %meeting_id,
                    model = %llm_model,
                    picked = ?other,
                    "model named no known template; using general"
                );
                TEMPLATE_IDS[0]
            }
        },
    };
    let llm_output = strip_prompt_scaffolding(body);

    // `merge_notes(notes, "")` returns the notes untouched, which would be
    // persisted as the "enhanced" document with no error, so fail loudly.
    if llm_output.trim().is_empty() {
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

    let merged = match merge_notes(&user_notes, &llm_output, &transcript_json) {
        Ok(m) => m,
        Err(e) => {
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
    let rendered_md = yogurt_notes::render::to_markdown(&merged);
    // Strip LLM-hallucinated script/img/iframe/on* handlers/javascript: URLs before
    // persisting. The wire-format spans survive.
    let enriched_md = crate::sanitize::sanitize_enriched_md(&rendered_md);
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

/// Drop lines where the model echoed the enhance prompt's scaffolding (section
/// wrappers, instruction parentheticals, `---` separators). Lines are judged with
/// `<...>` tags removed, so a header wrapped in marker spans is still caught.
fn strip_prompt_scaffolding(s: &str) -> String {
    fn without_tags(line: &str) -> String {
        let mut out = String::new();
        let mut depth = 0u32;
        for c in line.chars() {
            match c {
                '<' => depth += 1,
                '>' => depth = depth.saturating_sub(1),
                _ if depth == 0 => out.push(c),
                _ => {}
            }
        }
        out
    }
    s.lines()
        .filter(|line| {
            // Checked before tag-stripping, which would reduce these to kept blank lines.
            let raw = line.trim().to_lowercase();
            if matches!(
                raw.as_str(),
                "<user_notes>" | "</user_notes>" | "<transcript>" | "</transcript>"
            ) {
                return false;
            }
            let flat = without_tags(line);
            let flat = flat.trim();
            if flat.is_empty() {
                // Pure markup (e.g. an empty marker span) is dropped; real blanks are kept.
                return line.trim().is_empty();
            }
            let lower = flat.to_lowercase();
            // `---` separators: alone, as a bullet/heading, or followed only by a
            // `\u{21b3} mm:ss` transcript-link stub.
            let residue: String = flat
                .chars()
                .filter(|c| {
                    !matches!(c, '#' | '-' | '*' | ' ' | '\u{21b3}' | ':') && !c.is_ascii_digit()
                })
                .collect();
            let has_dashes = flat.contains("---");
            let is_separator =
                residue.is_empty() && (has_dashes || flat.trim_matches(['#', '*', ' ']).is_empty());
            let is_scaffold = lower.contains("user notes")
                && (lower.starts_with('#') || lower.contains("preserve verbatim"))
                || lower.starts_with("## transcript")
                || lower.contains("source for your additions");
            !(is_separator || is_scaffold)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn strip_model_reasoning(s: &str) -> String {
    yogurt_llm::strip_thinking(s)
}

/// Repair mojibake the model emits as literal tokens (observed from MiniMax in both
/// the CP1252 form "\u{e2}\u{20ac}\u{a6}" and the Latin-1 form with raw C1 controls).
/// Maps each char back to its source byte and decodes runs that form valid UTF-8.
/// ASCII and legitimate accented text pass through, since a lone "\u{e2}" followed
/// by ASCII never forms a valid sequence.
fn fix_model_mojibake(s: &str) -> String {
    fn mangled_byte(c: char) -> Option<u8> {
        let u = c as u32;
        if (0x80..=0xFF).contains(&u) {
            return Some(u as u8); // Latin-1 range, incl. raw C1 controls
        }
        // CP1252's 0x80-0x9F remappings.
        Some(match c {
            '\u{20AC}' => 0x80,
            '\u{201A}' => 0x82,
            '\u{0192}' => 0x83,
            '\u{201E}' => 0x84,
            '\u{2026}' => 0x85,
            '\u{2020}' => 0x86,
            '\u{2021}' => 0x87,
            '\u{02C6}' => 0x88,
            '\u{2030}' => 0x89,
            '\u{0160}' => 0x8A,
            '\u{2039}' => 0x8B,
            '\u{0152}' => 0x8C,
            '\u{017D}' => 0x8E,
            '\u{2018}' => 0x91,
            '\u{2019}' => 0x92,
            '\u{201C}' => 0x93,
            '\u{201D}' => 0x94,
            '\u{2022}' => 0x95,
            '\u{2013}' => 0x96,
            '\u{2014}' => 0x97,
            '\u{02DC}' => 0x98,
            '\u{2122}' => 0x99,
            '\u{0161}' => 0x9A,
            '\u{203A}' => 0x9B,
            '\u{0153}' => 0x9C,
            '\u{017E}' => 0x9E,
            '\u{0178}' => 0x9F,
            _ => return None,
        })
    }

    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let lead = mangled_byte(c).filter(|b| (0xC2..=0xF4).contains(b));
        if let Some(lead_byte) = lead {
            let need = match lead_byte {
                0xC2..=0xDF => 1,
                0xE0..=0xEF => 2,
                _ => 3,
            };
            let mut bytes = vec![lead_byte];
            for j in 1..=need {
                match chars.get(i + j).copied().and_then(mangled_byte) {
                    Some(b) if (0x80..=0xBF).contains(&b) => bytes.push(b),
                    _ => break,
                }
            }
            if bytes.len() == need + 1 {
                if let Ok(decoded) = std::str::from_utf8(&bytes) {
                    out.push_str(decoded);
                    i += need + 1;
                    continue;
                }
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{fix_model_mojibake, strip_model_reasoning};

    #[test]
    fn strips_model_reasoning_from_content() {
        let output =
            "<think>\nI should analyze the transcript.\n</think>\n\n## Decisions\n- Ship Monday";
        assert_eq!(strip_model_reasoning(output), "## Decisions\n- Ship Monday");
    }

    #[test]
    fn scrubs_cp1252_variant() {
        let got = fix_model_mojibake(
            "October.\u{e2}\u{20ac}\u{a6} then \u{e2}\u{20ac}\u{153}ship\u{e2}\u{20ac}\u{9d}",
        );
        assert_eq!(got, "October.\u{2026} then \u{201c}ship\u{201d}");
    }

    #[test]
    fn scrubs_latin1_control_variant() {
        let got = fix_model_mojibake("let\u{e2}\u{80}\u{99}s see and\u{e2}\u{80}\u{a6} done");
        assert_eq!(got, "let\u{2019}s see and\u{2026} done");
    }

    #[test]
    fn strips_echoed_prompt_scaffolding() {
        let raw = "## USER NOTES (preserve verbatim, do not wrap)\n\n                   <span data-ai-grey data-ts=\"3\">--- <span data-transcript-link data-ts=\"3\">\u{21b3} 00:03</span></span>\n\n                   ## TRANSCRIPT (source for your additions; ts_ms is millis since meeting start)\n\n                   - <span data-ai-grey data-ts=\"30\">12:00: Staff meeting <span data-transcript-link data-ts=\"30\">\u{21b3} 00:30</span></span>\n                   <user_notes>\n</user_notes>";
        let got = super::strip_prompt_scaffolding(raw);
        assert!(!got.contains("USER NOTES"), "got: {got}");
        assert!(!got.contains("TRANSCRIPT ("), "got: {got}");
        assert!(!got.contains("---"), "got: {got}");
        assert!(!got.contains("<user_notes>"), "got: {got}");
        assert!(
            got.contains("12:00: Staff meeting"),
            "real bullet kept: {got}"
        );
    }

    #[test]
    fn keeps_normal_headings_and_bullets() {
        let doc = "## Decisions\n- <span data-ai-grey data-ts=\"16\">Pro tier: $20/month</span>\n\n## Action items\n- follow up";
        assert_eq!(super::strip_prompt_scaffolding(doc), doc);
    }

    #[test]
    fn clean_text_passes_through_untouched() {
        let s = "normal text with real \u{2026} and \u{201c}quotes\u{201d} and \u{21b3} 00:16";
        assert_eq!(fix_model_mojibake(s), s);
        let fr = "c\u{e2}ble and caf\u{e9} \u{e0} Paris";
        assert_eq!(fix_model_mojibake(fr), fr);
    }
}
