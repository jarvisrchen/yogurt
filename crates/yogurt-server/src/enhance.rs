//! Phase 4 hero augmented-notes endpoint (CONTEXT D-22 / D-23).
//!
//! `POST /api/meetings/{id}/enhance` accepts the user's raw notes plus the
//! transcript-so-far in the request body, renders the bundled `enhance.md`
//! prompt, calls the LLM (env-var override, else the configured active
//! provider + Keychain key, else the deterministic `MockLlm` when nothing
//! is configured), runs `yogurt_notes::merge_notes` over
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

use crate::markdown_exporter::Meeting as ExpMeeting;
use crate::AppState;
use yogurt_llm::{ChatMessage, ChatRequest};
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
    // 1) Look up the meeting.
    //
    // HI-9: in-memory Registry is wiped on server restart, but Phase 4-03's
    // UPSERT persists the meeting row to SQLite on first enhance. After a
    // restart, a previously-enhanced meeting that the user reaches via
    // direct link / refresh would 404 on Re-enhance because the in-memory
    // lookup misses. Fall back to re-hydrating from SQLite: if the row
    // exists there, materialize a fresh in-memory Meeting (with empty
    // broadcasts — the user can't get live transcript on a stopped meeting
    // anyway, but Re-enhance still works because it only needs events_tx).
    let meeting = match state.meetings.get(&meeting_id).await {
        Some(m) => m,
        None => {
            // Probe SQLite via the read pool. If the row exists, register a
            // transient Meeting so the rest of the handler (including the
            // events_tx broadcast) operates normally.
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
            // Hydrate a fresh Meeting and insert it into the registry so
            // subsequent calls (including the WS broadcast subscribers) see
            // the same one. No audio/transcript broadcasts will ever fire —
            // recording is over — but events_tx works for enhance_progress.
            state.meetings.hydrate(meeting_id).await
        }
    };

    // 1b) Resolve the transcript. The server-side STT persistence task
    // (meetings.rs) is the source of truth for what was said; the client
    // no longer carries the transcript in the request body. Prefer the
    // stored row whenever the request's copy is empty so Re-enhance and
    // the normal end-of-meeting flow both see the real transcript.
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

    // 2) Render the user prompt.
    let user_prompt = state
        .prompts
        .render_enhance(&EnhanceCtx {
            notes: &req.notes_md,
            transcript: &transcript_json,
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

    // 4) Call the LLM via the `yogurt_llm::LlmClient` trait (Plan 05-01).
    //
    // Provider resolution is a three-tier priority chain:
    //   1. `YOGURT_LLM_*` env vars (developer override, highest priority).
    //   2. Active provider row (`providers` table) + its Keychain key via
    //      `llm_openai::from_active_provider`. A configured provider whose
    //      key can't be read is a hard 502 — never a silent mock fallback.
    //   3. `MockLlm` only when nothing is configured at all, so first-run
    //      users still get a working hero experience (logged as a warn).
    //
    // BL-5: wrap with a hard timeout. If reqwest's own timeout fires we
    // catch it via Err(...). If the future just stalls (e.g. an in-process
    // mock that .await's forever), tokio::time::timeout brings us back.
    // On either failure path emit `enhance_progress { phase: "error" }`
    // so the browser banner transitions to an error state instead of
    // spinning forever.
    //
    // System message is intentionally empty here — Phase 4's `enhance.md`
    // prompt is rendered as the whole user payload; introducing a real
    // system message requires moving the prompt split logic into
    // `yogurt-prompts`.
    let llm = match crate::llm_openai::resolve(&state).await {
        Ok(c) => c,
        Err(e) => {
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
        stream: false,
    };
    let llm_call = async {
        llm.complete(chat_req)
            .await
            .map(|r| r.content)
            .map_err(|e| (StatusCode::BAD_GATEWAY, format!("llm complete failed: {e}")))
    };
    let llm_output = match tokio::time::timeout(crate::llm_openai::LLM_HTTP_TIMEOUT, llm_call).await
    {
        Ok(Ok(out)) => out,
        Ok(Err((code, msg))) => {
            // LLM returned an error (HTTP non-2xx, parse failure, etc.).
            let _ = meeting.events_tx.send(serde_json::json!({
                "type": "enhance_progress",
                "phase": "error",
                "message": format!("Enhance failed: {msg}")
            }));
            return Err((code, msg));
        }
        Err(_elapsed) => {
            // tokio::time::timeout fired.
            let _ = meeting.events_tx.send(serde_json::json!({
                "type": "enhance_progress",
                "phase": "error",
                "message": "LLM timed out — try Re-enhance"
            }));
            return Err((
                StatusCode::GATEWAY_TIMEOUT,
                format!(
                    "llm call exceeded {}s timeout",
                    crate::llm_openai::LLM_HTTP_TIMEOUT.as_secs()
                ),
            ));
        }
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
    // BL-5: on merge failure (malformed user_md / enriched_md from a buggy
    // LLM) also emit `error` so the banner exits the spinner state.
    // Some models (observed live with MiniMax-Text-01) emit CP1252-mangled
    // punctuation as literal tokens — "â€¦" where "…" belongs. Our own
    // pipeline is UTF-8-clean end to end; this scrubs the model's junk so
    // the stored document isn't permanently ugly.
    let llm_output = fix_model_mojibake(&llm_output);

    let merged = match merge_notes(&req.notes_md, &llm_output, &transcript_json) {
        Ok(m) => m,
        Err(e) => {
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
    let transcript_json_for_blocking = transcript_json.clone();
    let enriched_md_for_blocking = enriched_md.clone();
    let enriched_doc_json_for_blocking = enriched_doc_json.clone();
    let title_for_blocking = title.clone();
    let meeting_id_for_blocking = meeting_id_str.clone();

    let notes_path =
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
        .await;

    // BL-5: surface persistence failures to the browser as an error event
    // so the EnhancingBanner exits its spinning state.
    let notes_path: PathBuf = match notes_path {
        Ok(Ok(p)) => p,
        Ok(Err((code, msg))) => {
            let _ = meeting.events_tx.send(serde_json::json!({
                "type": "enhance_progress",
                "phase": "error",
                "message": format!("Persistence failed: {msg}")
            }));
            return Err((code, msg));
        }
        Err(join_err) => {
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

    // 7c) Phase 7 (Plan 07-01): the Library REST surface (`GET /api/meetings`)
    // reads from `yogurt_db::MeetingRepo`, not from Phase 0 storage. Mirror
    // the post-enhance state (title, enriched_md, ended_at,
    // started_at, notes_md, transcript_json) into the Library directory so
    // the two layers don't diverge. Creates the row on the fly if the user
    // enhanced a meeting that was never seen by the Library (e.g.
    // pre-Phase-7 row). The repo bumps `updated_at` so the Library re-sorts.
    {
        let id = meeting_id_str.clone();
        let title = title.clone();
        let notes_md = req.notes_md.clone();
        let transcript_json = transcript_json.clone();
        let enriched_md_for_repo = enriched_md.clone();
        let started_at_fallback = started_at_unix_ms;
        // Timestamp tri-state: only touch the repo's timestamps when the
        // REQUEST explicitly carried them. The normal flow stamps
        // started_at on /start and ended_at on /stop — mapping an absent
        // request field to `Some(None)` here used to CLEAR the ended_at
        // that /stop had just written, which is why every meeting showed
        // a dash duration in the library.
        let started_at_patch = req.started_at_unix_ms;
        let ended_at_patch = req.ended_at_unix_ms.map(Some);
        let state2 = state.clone();
        let _ = tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            // Ensure the row exists before patching — older meetings
            // pre-date the Library surface entirely.
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
                },
            )?;
            Ok(())
        })
        .await;
    }

    // 8) Emit `enhance_progress` — done.
    let _ = meeting
        .events_tx
        .send(serde_json::json!({"type": "enhance_progress", "phase": "done"}));

    Ok(Json(EnhanceResponse {
        enriched_md,
        notes_file: notes_path.to_string_lossy().into_owned(),
    }))
}

/// Repair CP1252-double-encoded punctuation that some models emit as
/// literal tokens (UTF-8 bytes of "…" read back as Latin-1 during the
/// model's own training/serving — not produced by our pipeline). Longer
/// sequences first so "â€¦" wins over any "â€" prefix rule.
fn fix_model_mojibake(s: &str) -> String {
    const MAP: &[(&str, &str)] = &[
        ("\u{c3}\u{a2}\u{20ac}\u{a6}", "\u{2026}"),   // â€¦ → …
        ("\u{c3}\u{a2}\u{20ac}\u{201d}", "\u{201d}"), // â€ + ” → ”
        ("\u{c3}\u{a2}\u{20ac}\u{153}", "\u{201c}"),  // â€œ → “
        ("\u{c3}\u{a2}\u{20ac}\u{2122}", "\u{2019}"), // â€™ → ’
        ("\u{c3}\u{a2}\u{20ac}\u{201c}", "\u{2013}"), // â€“ → –
        ("\u{c3}\u{a2}\u{20ac}\u{201e}", "\u{2014}"), // â€„variant → —
        ("\u{e2}\u{20ac}\u{a6}", "\u{2026}"),         // â€¦ (bare) → …
        ("\u{e2}\u{20ac}\u{2122}", "\u{2019}"),       // â€™ (bare) → ’
        ("\u{e2}\u{20ac}\u{153}", "\u{201c}"),        // â€œ (bare) → “
        ("\u{e2}\u{20ac}\u{9d}", "\u{201d}"),         // â€ +9d (bare) → ”
        ("\u{e2}\u{20ac}\u{201c}", "\u{2013}"),       // â€“ (bare) → –
        ("\u{c2}\u{a0}", " "),                        // Â  (nbsp mangle) → space
    ];
    let mut out = s.to_string();
    for (bad, good) in MAP {
        if out.contains(bad) {
            out = out.replace(bad, good);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::fix_model_mojibake;

    #[test]
    fn scrubs_cp1252_mangled_ellipsis_and_quotes() {
        let got = fix_model_mojibake(
            "launch in October.\u{e2}\u{20ac}\u{a6} then \u{e2}\u{20ac}\u{153}ship\u{e2}\u{20ac}\u{9d}",
        );
        assert_eq!(got, "launch in October.\u{2026} then \u{201c}ship\u{201d}");
    }

    #[test]
    fn clean_text_passes_through_untouched() {
        let s = "normal text with real … and “quotes” and ↳ 00:16";
        assert_eq!(fix_model_mojibake(s), s);
    }
}
