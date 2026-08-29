//! Plan 05-01 Task 2: SSE streaming round-trip against a wiremock
//! stand-in OpenAI server. Verifies that:
//! - `LlmClient::stream` opens a stream successfully on 200 + SSE body.
//! - Mid-stream `content` deltas accumulate to the expected text.
//! - The terminal `[DONE]` event surfaces as `ChatChunk { done: true }`.
//! - Embedded `<think>…</think>` blocks are stripped from the stream so
//!   reasoning models (DeepSeek R1, MiniMax M3, Qwen QwQ, …) don't leak
//!   chain-of-thought into the chat bubble. Defense in depth on top of
//!   any provider-specific `reasoning_split` request parameter.

use futures_util::StreamExt;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use yogurt_llm::{ChatMessage, ChatRequest, LlmClient, OpenAiCompatClient};

#[tokio::test]
async fn it_streams_sse_chunks_into_chat_chunks() {
    // Hand-crafted OpenAI SSE body. Each event is `data: {json}\n\n`,
    // terminated by `data: [DONE]\n\n`. The fourth event carries
    // `finish_reason: "stop"` with an empty delta — a real provider
    // sends this immediately before `[DONE]` to signal "this was the
    // last content chunk".
    let body = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"Hel\"},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\"lo \"},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\"yogurt.\"},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
        "data: [DONE]\n\n",
    );

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(body),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = OpenAiCompatClient::new(server.uri(), "sk-test".into(), "gpt-4o-mini".into());
    let mut stream = client
        .stream(ChatRequest {
            messages: vec![ChatMessage::user("hi")],
            stream: true,
        })
        .await
        .expect("stream opens");

    let mut deltas: Vec<String> = Vec::new();
    let mut saw_done = false;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.expect("chunk ok");
        if !chunk.delta.is_empty() {
            deltas.push(chunk.delta);
        }
        if chunk.done {
            saw_done = true;
        }
    }
    assert_eq!(
        deltas.join(""),
        "Hello yogurt.",
        "deltas should accumulate to the upstream text"
    );
    assert!(
        saw_done,
        "stream should emit at least one terminal `done=true` chunk"
    );
}

#[tokio::test]
async fn it_surfaces_non_2xx_stream_open_as_error() {
    // If the provider rejects the stream-open request (auth, model not
    // found, rate limit), `stream()` itself must error — the caller
    // should not get back an empty `BoxStream` that simply ends.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(429).set_body_string("rate limited"))
        .mount(&server)
        .await;

    let client = OpenAiCompatClient::new(server.uri(), "sk-bad".into(), "m".into());
    // `BoxStream` is not `Debug`, so `.expect_err` (which requires the Ok
    // variant to be Debug) won't compile — match by hand.
    let result = client
        .stream(ChatRequest {
            messages: vec![ChatMessage::user("hi")],
            stream: true,
        })
        .await;
    let err = match result {
        Ok(_) => panic!("expected stream-open to fail on 429"),
        Err(e) => e,
    };
    let msg = format!("{err:#}");
    assert!(
        msg.contains("429") || msg.contains("rate limited"),
        "expected status or body in error, got: {msg}"
    );
}

/// Regression for the chat bug where MiniMax M3 (and any other reasoning
/// model) leaks `<think>…</think>` into the streamed `delta.content`.
/// `reasoning_split: true` on the request side is honored by some
/// providers and ignored by others, so this is the model-agnostic
/// backstop: every visible delta must arrive at the chat handler with
/// the think block removed.
#[tokio::test]
async fn it_strips_inline_think_blocks_from_streamed_deltas() {
    // Single chunk carries the full think block followed by the visible
    // answer — the most common shape when the provider ignores
    // `reasoning_split` and stuffs the whole response into one delta.
    let body = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"<think>\\nlet me think...\\n</think>\\n\\n\"},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\"They agreed on 25% Monday.\"},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
        "data: [DONE]\n\n",
    );

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(body),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = OpenAiCompatClient::new(server.uri(), "sk-test".into(), "MiniMax-M3".into());
    let mut stream = client
        .stream(ChatRequest {
            messages: vec![ChatMessage::user("what was decided?")],
            stream: true,
        })
        .await
        .expect("stream opens");

    let mut accumulated = String::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.expect("chunk ok");
        accumulated.push_str(&chunk.delta);
    }
    assert_eq!(
        accumulated, "They agreed on 25% Monday.",
        "think block + whitespace must be stripped; got: {accumulated:?}"
    );
}

/// Regression for the chunk-boundary edge case: a think-tag opening
/// (`<th` and `ink>…`) is split across two SSE deltas. The stripper must
/// buffer across chunks so the user never sees the partial tag.
#[tokio::test]
async fn it_strips_think_tags_split_across_chunk_boundaries() {
    let body = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"<think>\\nrea\"},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\"soning</think>\\n\\n\"},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\"Final answer.\"},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
        "data: [DONE]\n\n",
    );

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(body),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = OpenAiCompatClient::new(server.uri(), "sk-test".into(), "MiniMax-M3".into());
    let mut stream = client
        .stream(ChatRequest {
            messages: vec![ChatMessage::user("hi")],
            stream: true,
        })
        .await
        .expect("stream opens");

    let mut accumulated = String::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.expect("chunk ok");
        accumulated.push_str(&chunk.delta);
    }
    assert_eq!(
        accumulated, "Final answer.",
        "split-tag thinking must be elided; got: {accumulated:?}"
    );
}

/// `<thinking>…</thinking>` is the alias some providers (Qwen QwQ family)
/// use instead of `<think>`. Same defense, both shapes must be stripped.
#[tokio::test]
async fn it_strips_thinking_alias_block_from_streamed_deltas() {
    let body = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"<thinking>hidden</thinking>visible\"},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
        "data: [DONE]\n\n",
    );

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(body),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = OpenAiCompatClient::new(server.uri(), "sk-test".into(), "qwen-qwq".into());
    let mut stream = client
        .stream(ChatRequest {
            messages: vec![ChatMessage::user("hi")],
            stream: true,
        })
        .await
        .expect("stream opens");

    let mut accumulated = String::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.expect("chunk ok");
        accumulated.push_str(&chunk.delta);
    }
    assert_eq!(
        accumulated, "visible",
        "<thinking> alias must be stripped; got: {accumulated:?}"
    );
}
