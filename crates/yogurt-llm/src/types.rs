//! Public chat types + private OpenAI wire types for the
//! `LlmClient` trait surface (CONTEXT D-01, D-02).
//!
//! Splitting wire-types into a private submodule keeps the OpenAI shape
//! (`role: String` strings, snake_case fields, optional deltas, …) out of
//! the public API — Phase 8 will implement the same trait against
//! whisper-rs-shaped local models and shouldn't be forced to mirror
//! OpenAI's request/response JSON.

use serde::{Deserialize, Serialize};

/// One message in a chat-completion request. Role is a free-form string
/// because OpenAI-compatible providers accept `system` / `user` /
/// `assistant` / `tool` / `function` / `developer` (o1-preview). Phase 6's
/// chat handler will round-trip these directly to the model — we don't
/// constrain the role here.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    /// `"system"` | `"user"` | `"assistant"` (and provider-specific roles).
    pub role: String,
    pub content: String,
}

impl ChatMessage {
    /// Convenience constructor for a `system`-role message (typically used
    /// for the enhance.md prompt body).
    pub fn system(s: impl Into<String>) -> Self {
        Self {
            role: "system".into(),
            content: s.into(),
        }
    }

    /// Convenience constructor for a `user`-role message (the merged
    /// NOTES + TRANSCRIPT payload, or a chat user turn).
    pub fn user(s: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: s.into(),
        }
    }

    /// Convenience constructor for an `assistant`-role message — used by
    /// Phase 6's chat handler when feeding history back to the model.
    pub fn assistant(s: impl Into<String>) -> Self {
        Self {
            role: "assistant".into(),
            content: s.into(),
        }
    }
}

/// Caller-supplied chat-completion request shape.
///
/// `stream` is descriptive only — `LlmClient::complete` always sets
/// `stream=false` on the wire and `LlmClient::stream` always sets
/// `stream=true`. The field exists so a single value can be threaded
/// through both code paths (Phase 4 enhance uses non-streaming; Phase 6
/// chat will flip the same value to `true`).
#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub messages: Vec<ChatMessage>,
    pub stream: bool,
}

/// Result of a non-streaming `complete()` call.
///
/// `model` echoes the upstream-reported model string (some providers
/// return e.g. `gpt-4o-mini-2024-07-18` even when asked for `gpt-4o-mini`
/// — useful for logging / cost attribution).
#[derive(Debug, Clone)]
pub struct ChatResponse {
    pub content: String,
    pub model: String,
}

/// One SSE chunk in a streaming completion.
///
/// `delta` is the incremental text since the last chunk (empty for the
/// terminal `[DONE]` event and for chunks that only carry tool-call
/// metadata). `done` is `true` for the terminal chunk **and** for chunks
/// where the upstream API set `finish_reason` to a non-null value — the
/// browser-side accumulator should treat either flag as end-of-stream.
#[derive(Debug, Clone)]
pub struct ChatChunk {
    pub delta: String,
    pub done: bool,
}

// ─── Private wire types — exact OpenAI shape, used by lib.rs + streaming.rs ──
// Marked `pub(crate)` so the sibling `streaming` module can construct + parse
// them without re-export. NEVER expose these on the crate's public surface;
// any change to the OpenAI shape (new optional field, role rename) should be
// absorbed here without breaking downstream callers.

#[derive(Serialize)]
pub(crate) struct OpenAiRequest<'a> {
    pub model: &'a str,
    pub messages: &'a [ChatMessage],
    pub stream: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub reasoning_split: bool,
}

#[derive(Deserialize)]
pub(crate) struct OpenAiResponse {
    pub model: String,
    pub choices: Vec<OpenAiChoice>,
}

#[derive(Deserialize)]
pub(crate) struct OpenAiChoice {
    pub message: OpenAiMessage,
}

#[derive(Deserialize)]
pub(crate) struct OpenAiStreamChunk {
    pub choices: Vec<OpenAiStreamChoice>,
}

#[derive(Deserialize)]
pub(crate) struct OpenAiStreamChoice {
    pub delta: OpenAiDelta,
    /// `null` mid-stream; `"stop"` / `"length"` / `"tool_calls"` on the
    /// final content-bearing chunk. We treat `Some(_)` as "this is the
    /// last delta with content" and emit `done=true` alongside whatever
    /// final text the model produced.
    #[serde(default)]
    pub finish_reason: Option<String>,
}

/// Assistant message shape. Some reasoning providers (DeepSeek R1,
/// MiniMax M3 variants) carry chain-of-thought in a sibling
/// `reasoning_content` field instead of (or in addition to) inline
/// `<think>…</think>` blocks in `content`. The stripper in
/// `crate::thinking` handles the inline case; here we parse the
/// sibling field and explicitly discard it so it can never reach the
/// caller's `ChatResponse`.
#[derive(Deserialize)]
pub(crate) struct OpenAiMessage {
    #[serde(default)]
    pub role: String,
    pub content: String,
    /// Parsed and intentionally discarded — see field doc on the
    /// struct. `#[serde(default)]` keeps deserialization working for
    /// providers that don't emit the field.
    #[serde(default)]
    #[allow(dead_code)]
    pub reasoning_content: Option<String>,
}

impl From<OpenAiMessage> for ChatMessage {
    fn from(m: OpenAiMessage) -> Self {
        // `role` falls back to "assistant" when the provider omits it;
        // matches what we'd default to in the public constructor.
        let role = if m.role.is_empty() {
            "assistant".to_string()
        } else {
            m.role
        };
        ChatMessage {
            role,
            content: m.content,
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct OpenAiDelta {
    /// Optional because chunks carrying only `role` (first chunk in many
    /// providers' streams) or only tool-call metadata have no text.
    #[serde(default)]
    pub content: Option<String>,
    /// Parsed and intentionally discarded — see [`OpenAiMessage`] for
    /// the field semantics. Same sibling-reasoning field as the
    /// non-streaming shape; some providers (DeepSeek) emit it per
    /// delta. The streaming stripper already elides `<think>…</think>`
    /// blocks inside `content`, so this field is here purely to make
    /// the "we know about it, we drop it" intent explicit.
    #[serde(default)]
    #[allow(dead_code)]
    pub reasoning_content: Option<String>,
}

/// `GET /v1/models` response shape (OpenAI-compatible). `data` is the
/// list of model descriptors; `object` is metadata we ignore.
#[derive(Deserialize)]
pub(crate) struct ModelsResponse {
    pub data: Vec<ModelDescriptor>,
}

#[derive(Deserialize)]
pub(crate) struct ModelDescriptor {
    pub id: String,
}
