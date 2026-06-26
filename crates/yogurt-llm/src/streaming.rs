//! SSE streaming for `OpenAiCompatClient` (Plan 05-01 Task 2).
//!
//! Pipeline: `reqwest::Response::bytes_stream()` →
//! `eventsource_stream::Eventsource` adapter → `StreamExt::map` projection
//! into `ChatChunk`. The terminal `[DONE]` SSE event maps to
//! `ChatChunk { delta: "", done: true }`; mid-stream chunks emit the
//! delta text and set `done` to whether the upstream chunk's
//! `finish_reason` is non-null.
//!
//! Why we don't use the `async-openai` crate even though PRD §STACK calls
//! it out: that crate is excellent for the OpenAI API surface specifically
//! but its types are coupled to OpenAI's struct names. Our adapter needs
//! to work against Minimax, Ollama, LM Studio, vLLM, llama.cpp server,
//! OpenRouter, Groq, Together, Fireworks — all of which speak roughly
//! the same JSON but with provider-specific extensions. Hand-rolling the
//! 50 LOC of `reqwest + eventsource-stream` keeps the surface narrow and
//! the dependency list short.

use crate::{types, ChatChunk, ChatRequest, OpenAiCompatClient};
use anyhow::{anyhow, Result};
use eventsource_stream::Eventsource;
use futures_util::stream::{BoxStream, StreamExt};

pub(crate) async fn stream(
    client: &OpenAiCompatClient,
    req: ChatRequest,
) -> Result<BoxStream<'static, Result<ChatChunk>>> {
    let body = types::OpenAiRequest {
        model: client.model_for_streaming(),
        messages: &req.messages,
        stream: true,
    };

    let resp = client
        .http_for_streaming()
        .post(format!(
            "{}/chat/completions",
            client.base_url_for_streaming()
        ))
        .bearer_auth(client.api_key_for_streaming())
        .json(&body)
        .send()
        .await?;

    let status = resp.status();
    if !status.is_success() {
        // Drain body for the operator-actionable error (rate limit,
        // invalid model, account suspended). The Phase 4 enhance handler
        // converts this into an `enhance_progress { phase: "error" }`
        // WebSocket event.
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow!("LLM stream open failed: {status} — {body}"));
    }

    let byte_stream = resp.bytes_stream();
    let events = byte_stream.eventsource();

    // `events` is the SSE event stream; map each event into our public
    // `ChatChunk`. We box the result so the caller gets a single concrete
    // `BoxStream<'static, ...>` type regardless of internal pipeline
    // shape (the boxed stream is `Send + 'static` so it can be spawned
    // onto a tokio task, e.g. by Phase 6's chat WebSocket handler).
    let mapped = events.map(|ev| -> Result<ChatChunk> {
        let ev = ev.map_err(|e| anyhow!("SSE parse error: {e}"))?;
        // OpenAI terminates streams with `data: [DONE]` — emit a final
        // chunk with `done=true` and empty delta so accumulators close
        // cleanly. Some providers also send a content-bearing chunk with
        // `finish_reason` set just before `[DONE]`; that chunk emits its
        // own `done=true` and the `[DONE]` emits a second `done=true` —
        // accumulator-side this is idempotent.
        if ev.data.trim() == "[DONE]" {
            return Ok(ChatChunk {
                delta: String::new(),
                done: true,
            });
        }
        let chunk: types::OpenAiStreamChunk = serde_json::from_str(&ev.data)
            .map_err(|e| anyhow!("invalid chunk JSON: {e} — payload: {}", ev.data))?;
        let (delta, done) = match chunk.choices.into_iter().next() {
            Some(choice) => (
                choice.delta.content.unwrap_or_default(),
                choice.finish_reason.is_some(),
            ),
            // No choices means a heartbeat / metadata-only frame.
            // Surface as an empty mid-stream chunk so the caller can
            // tick its activity timer without treating it as EOS.
            None => (String::new(), false),
        };
        Ok(ChatChunk { delta, done })
    });

    Ok(mapped.boxed())
}
