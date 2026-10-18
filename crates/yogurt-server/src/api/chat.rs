//! `/api/meetings/:id/chat` REST surface and the `spawn_stream` LLM-to-WS bridge.
//!
//! `POST` returns the assistant placeholder's `message_id` immediately.
//! The token stream is fanned out as `chat_chunk` frames on the per-meeting `events_tx`.
//! A failed stream is persisted as the assistant content.

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

/// Empty list, not 404, for a meeting with no chat yet.
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

/// Returns immediately. The work runs on a detached tokio task.
pub async fn spawn_stream(state: AppState, meeting_id: Uuid, message_id: String) {
    tokio::spawn(async move {
        run_stream(state, meeting_id, message_id).await;
    });
}

async fn run_stream(state: AppState, meeting_id: Uuid, message_id: String) {
    let meeting_str = meeting_id.to_string();

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

    // yogurt-db has no transcript accessor, so read `meetings.transcript_json` directly.
    let transcript = read_transcript(&state, &meeting_str)
        .await
        .unwrap_or_default();

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
            // The empty assistant placeholder from post_chat.
            continue;
        }
        let m = match h.role {
            Role::User => LlmMessage::user(h.content.clone()),
            Role::Assistant => LlmMessage::assistant(h.content.clone()),
        };
        messages.push(m);
    }

    // Same resolution chain as enhance, so chat never answers from a different model than Settings.
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
                    // The upstream API may emit metadata after `finish_reason`.
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
            // Chat is allowed against ended meetings too.
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
    // No live subscribers is fine: the content is persisted and reload re-renders it.
    let _ = meeting.events_tx.send(value);
}

/// Render `[{ts_ms, channel, text}]` as `[mm:ss] speaker: text` lines.
/// Falls back to the input verbatim if it doesn't parse.
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
