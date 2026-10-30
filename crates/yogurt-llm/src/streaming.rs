use crate::thinking::ThinkStripper;
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
        reasoning_split: client.split_reasoning(),
        thinking: client.thinking(),
    };

    let resp = crate::send(
        client
            .http_for_streaming()
            .post(format!(
                "{}/chat/completions",
                client.base_url_for_streaming()
            ))
            .bearer_auth(client.api_key_for_streaming())
            .json(&body),
    )
    .await?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow!(
            "LLM stream open failed: {status} - {}",
            client.scrub(&body)
        ));
    }

    let byte_stream = resp.bytes_stream();
    let events = byte_stream.eventsource();

    let mapped = events.map(|ev| -> Result<ChatChunk> {
        let ev = ev.map_err(|e| anyhow!("SSE parse error: {e}"))?;
        // `[DONE]` terminates the stream. A `finish_reason` chunk may already have set `done`, so consumers must treat repeats as idempotent.
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
            // Heartbeat or metadata-only frame: empty mid-stream chunk so the caller can tick its activity timer.
            None => (String::new(), false),
        };
        Ok(ChatChunk { delta, done })
    });

    // Stateful so a tag split across chunk boundaries is still stripped.
    let mut stripper = ThinkStripper::new();
    let stripped = mapped.map(move |chunk_res| {
        let mut chunk = chunk_res?;
        chunk.delta = stripper.push(&chunk.delta);
        if chunk.done {
            chunk.delta.push_str(&stripper.flush());
        }
        Ok(chunk)
    });

    Ok(stripped.boxed())
}
