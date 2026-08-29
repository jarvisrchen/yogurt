//! Phase 6 (Plan 06-01) — `/api/meetings/:id/chat` REST surface and the
//! `spawn_stream` LLM-to-WS bridge.
//!
//! ## Wire contract
//!
//! - `POST /api/meetings/{id}/chat` body `{ "content": "<user text>" }` →
//!   `{ "message_id": "<26-char ULID>" }`. Returns within 100 ms; the
//!   actual LLM token stream is fanned out asynchronously via the
//!   per-meeting `events_tx` broadcast (`ws.rs`).
//! - `GET /api/meetings/{id}/chat` → `{ "messages": [ChatMessage, ...] }`
//!   in chronological order. Used by the frontend on remount to re-hydrate
//!   the chat window.
//!
//! ## Streaming flow (`spawn_stream`)
//!
//! 1. Insert one user row + one EMPTY assistant placeholder row. Capture the
//!    placeholder's ULID — that becomes `message_id` on every chunk.
//! 2. Read the system prompt via `state.prompts.chat_system()` (Phase 4
//!    `yogurt-prompts` accessor — no inline prompt anywhere).
//! 3. Read transcript JSON via the Phase 0 storage read pool (column
//!    `transcript_json` on `meetings`). Fallback to empty string with a
//!    `tracing::warn!` on any DB error — the chat still works without it.
//! 4. Read chat history via `Db::list_chat_messages`. Filter out the empty
//!    assistant placeholder so we don't feed an empty turn back to the model.
//! 5. Build `Vec<ChatMessage>` and call `state.llm.stream(ChatRequest { ... })`.
//!    On error, broadcast a single `chat_chunk { delta: "[stream error: …]",
//!    done: true }` and persist that text as the assistant content.
//! 6. For each chunk: accumulate the delta, broadcast a `chat_chunk` frame
//!    (`done: false`). After the stream finishes, persist the accumulated
//!    text + broadcast the terminal `done: true` chunk (delta empty).

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::ws::WsEvent;
use crate::AppState;
use futures_util::StreamExt;
use yogurt_db::chat::{ChatMessage, Role};
use yogurt_llm::{ChatMessage as LlmMessage, ChatRequest};

#[derive(Debug, Deserialize)]
pub struct ChatRequestBody {
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct ChatResponseBody {
    pub message_id: String,
}

#[derive(Debug, Serialize)]
pub struct ChatHistoryResponse {
    pub messages: Vec<ChatMessage>,
}

/// `POST /api/meetings/{id}/chat` — insert user + placeholder assistant,
/// kick off the async LLM stream, return the assistant `message_id`.
pub async fn post_chat(
    State(state): State<AppState>,
    Path(meeting_id): Path<Uuid>,
    Json(req): Json<ChatRequestBody>,
) -> Response {
    let content = req.content.trim();
    if content.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "content must not be empty" })),
        )
            .into_response();
    }
    let meeting_str = meeting_id.to_string();

    let user_msg = ChatMessage::new(&meeting_str, Role::User, content.to_string());
    if let Err(e) = state.db.insert_chat_message(&user_msg) {
        tracing::error!(error = %e, "chat: failed to insert user message");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("db insert failed: {e}") })),
        )
            .into_response();
    }
    let assistant_msg = ChatMessage::new(&meeting_str, Role::Assistant, String::new());
    if let Err(e) = state.db.insert_chat_message(&assistant_msg) {
        tracing::error!(error = %e, "chat: failed to insert assistant placeholder");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("db insert failed: {e}") })),
        )
            .into_response();
    }

    let assistant_id = assistant_msg.id.clone();
    spawn_stream(state, meeting_id, assistant_id.clone()).await;

    (
        StatusCode::OK,
        Json(ChatResponseBody {
            message_id: assistant_id,
        }),
    )
        .into_response()
}

/// `GET /api/meetings/{id}/chat` — return full chat history in chronological
/// order. Empty list (not 404) for a meeting with no chat yet.
pub async fn get_chat_history(
    State(state): State<AppState>,
    Path(meeting_id): Path<Uuid>,
) -> Response {
    let meeting_str = meeting_id.to_string();
    match state.db.list_chat_messages(&meeting_str) {
        Ok(messages) => (StatusCode::OK, Json(ChatHistoryResponse { messages })).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "chat: failed to list history");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("db query failed: {e}") })),
            )
                .into_response()
        }
    }
}

/// Spawn the LLM streaming task. Always returns immediately — the actual
/// work runs on a detached tokio task.
///
/// Public-in-crate for the integration tests in `tests/chat_streaming.rs`
/// that want to assert against the spawn-and-stream behavior directly.
pub async fn spawn_stream(state: AppState, meeting_id: Uuid, message_id: String) {
    tokio::spawn(async move {
        run_stream(state, meeting_id, message_id).await;
    });
}

async fn run_stream(state: AppState, meeting_id: Uuid, message_id: String) {
    let meeting_str = meeting_id.to_string();

    // System prompt via Phase 4 prompts accessor — NOT hardcoded here.
    let system_prompt = match state.prompts.chat_system() {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(error = %e, "chat: failed to load chat-system.md");
            broadcast_error(
                &state,
                meeting_id,
                &message_id,
                &format!("prompt load failed: {e}"),
            )
            .await;
            return;
        }
    };

    // Transcript: stored on `meetings.transcript_json`. The Phase 5 yogurt-db
    // surface does NOT expose a `get_meeting_transcript` accessor — Phase 7
    // (library) will introduce one. Until then we read directly via the
    // Phase 0 storage read-pool and fall back to empty on any error.
    let transcript = read_transcript(&state, &meeting_str)
        .await
        .unwrap_or_default();

    // History: filter out the empty placeholder we just inserted (no point
    // feeding the model its own empty turn).
    let history = match state.db.list_chat_messages(&meeting_str) {
        Ok(h) => h,
        Err(e) => {
            tracing::error!(error = %e, "chat: failed to load history");
            broadcast_error(
                &state,
                meeting_id,
                &message_id,
                &format!("history load failed: {e}"),
            )
            .await;
            return;
        }
    };

    let mut messages: Vec<LlmMessage> = Vec::with_capacity(history.len() + 2);
    messages.push(LlmMessage::system(system_prompt));
    messages.push(LlmMessage::system(format!(
        "TRANSCRIPT SO FAR (most recent at bottom):\n\n{}",
        transcript_lines(&transcript)
    )));
    for h in &history {
        if h.id == message_id {
            // Skip the empty assistant placeholder we minted in post_chat.
            continue;
        }
        let m = match h.role {
            Role::User => LlmMessage::user(h.content.clone()),
            Role::Assistant => LlmMessage::assistant(h.content.clone()),
        };
        messages.push(m);
    }

    // Resolve the LLM per-request (env override → active provider +
    // stored key → mock) — the exact same chain the enhance handler uses,
    // so chat can never silently answer from a different model than the
    // one the user configured in Settings.
    let llm = match crate::llm_openai::resolve(&state).await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "chat: LLM resolution failed");
            broadcast_error(&state, meeting_id, &message_id, &format!("[{e}]")).await;
            return;
        }
    };

    let stream_res = llm
        .stream(ChatRequest {
            messages,
            stream: true,
        })
        .await;
    let mut stream = match stream_res {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, "chat: stream open failed");
            broadcast_error(
                &state,
                meeting_id,
                &message_id,
                &format!("[stream error: {e}]"),
            )
            .await;
            return;
        }
    };

    let mut accumulated = String::new();
    while let Some(chunk_res) = stream.next().await {
        match chunk_res {
            Ok(chunk) => {
                if !chunk.delta.is_empty() {
                    accumulated.push_str(&chunk.delta);
                    send_event(
                        &state,
                        meeting_id,
                        WsEvent::ChatChunk {
                            message_id: message_id.clone(),
                            delta: chunk.delta,
                            done: false,
                        },
                    )
                    .await;
                }
                if chunk.done {
                    // Stop reading further — the upstream API may emit
                    // metadata after `finish_reason`, but the user-visible
                    // text is complete.
                    break;
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "chat: stream chunk error");
                let err_text = format!("\n\n[stream error: {e}]");
                accumulated.push_str(&err_text);
                send_event(
                    &state,
                    meeting_id,
                    WsEvent::ChatChunk {
                        message_id: message_id.clone(),
                        delta: err_text,
                        done: false,
                    },
                )
                .await;
                break;
            }
        }
    }

    if let Err(e) = state
        .db
        .update_chat_message_content(&message_id, &accumulated)
    {
        tracing::error!(error = %e, "chat: failed to persist accumulated content");
    }

    send_event(
        &state,
        meeting_id,
        WsEvent::ChatChunk {
            message_id,
            delta: String::new(),
            done: true,
        },
    )
    .await;
}

async fn broadcast_error(state: &AppState, meeting_id: Uuid, message_id: &str, err: &str) {
    let body = format!("\n\n{err}");
    if let Err(e) = state.db.update_chat_message_content(message_id, &body) {
        tracing::warn!(error = %e, "chat: failed to persist error placeholder");
    }
    send_event(
        state,
        meeting_id,
        WsEvent::ChatChunk {
            message_id: message_id.to_string(),
            delta: body,
            done: true,
        },
    )
    .await;
}

async fn send_event(state: &AppState, meeting_id: Uuid, event: WsEvent) {
    let meeting = match state.meetings.get(&meeting_id).await {
        Some(m) => m,
        None => {
            // Hydrate so the WS handler can attach even if the meeting is
            // post-recording. The chat endpoint is allowed against any
            // known meeting id, including ones that have already ended.
            state.meetings.hydrate(meeting_id).await
        }
    };
    let value = match serde_json::to_value(&event) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "chat: failed to serialize WsEvent");
            return;
        }
    };
    // Ignore send errors — a meeting with no live WS subscribers is fine
    // (the chunks are persisted to the DB; reload re-renders the bubble).
    let _ = meeting.events_tx.send(value);
}

/// Render stored transcript JSON (`[{ts_ms, channel, text}]`) as readable
/// `[mm:ss] speaker: text` lines for the chat prompt. The model answers
/// questions about the meeting — prose beats raw JSON here. Falls back to
/// the input verbatim if it doesn't parse (older rows, tests).
fn transcript_lines(transcript_json: &str) -> String {
    #[derive(serde::Deserialize)]
    struct Seg {
        ts_ms: u64,
        channel: String,
        text: String,
    }
    let Ok(segs) = serde_json::from_str::<Vec<Seg>>(transcript_json) else {
        return transcript_json.to_string();
    };
    if segs.is_empty() {
        return "(no transcript captured yet)".to_string();
    }
    segs.iter()
        .map(|s| {
            let total_s = s.ts_ms / 1000;
            let speaker = match s.channel.as_str() {
                "me" | "mic" => "me",
                _ => "them",
            };
            format!(
                "[{:02}:{:02}] {speaker}: {}",
                total_s / 60,
                total_s % 60,
                s.text
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

async fn read_transcript(state: &AppState, meeting_id: &str) -> Option<String> {
    let reader = state.storage.read();
    let meeting_id_owned = meeting_id.to_string();
    let result = tokio::task::spawn_blocking(move || {
        let conn = match reader.lock() {
            Ok(c) => c,
            Err(e) => return Err(format!("db lock: {e}")),
        };
        conn.query_row(
            "SELECT transcript_json FROM meetings WHERE id = ?1",
            rusqlite::params![meeting_id_owned],
            |r| r.get::<_, Option<String>>(0),
        )
        .map_err(|e| format!("db query: {e}"))
    })
    .await;
    match result {
        Ok(Ok(opt)) => opt,
        Ok(Err(e)) => {
            tracing::warn!(error = %e, "chat: transcript read failed; falling back to empty");
            None
        }
        Err(e) => {
            tracing::warn!(error = %e, "chat: transcript read task join failed");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::transcript_lines;

    #[test]
    fn renders_segments_as_speaker_lines() {
        let json = r#"[{"ts_ms": 63000, "channel": "me", "text": "Let's talk pricing."},
                       {"ts_ms": 71000, "channel": "them", "text": "Twenty dollars a month."}]"#;
        assert_eq!(
            transcript_lines(json),
            "[01:03] me: Let's talk pricing.\n[01:11] them: Twenty dollars a month."
        );
    }

    #[test]
    fn empty_transcript_says_so() {
        assert_eq!(transcript_lines("[]"), "(no transcript captured yet)");
    }

    #[test]
    fn unparseable_input_passes_through() {
        assert_eq!(transcript_lines("not json"), "not json");
    }
}
