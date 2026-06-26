//! Plan 05-01 Task 2: SSE streaming round-trip against a wiremock
//! stand-in OpenAI server. Verifies that:
//! - `LlmClient::stream` opens a stream successfully on 200 + SSE body.
//! - Mid-stream `content` deltas accumulate to the expected text.
//! - The terminal `[DONE]` event surfaces as `ChatChunk { done: true }`.

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
