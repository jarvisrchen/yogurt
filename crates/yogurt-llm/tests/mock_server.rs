//! Plan 05-01 Task 1 acceptance tests for the non-streaming
//! `OpenAiCompatClient` (`LlmClient::complete`) against a wiremock
//! stand-in OpenAI server.

use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use yogurt_llm::{ChatMessage, ChatRequest, LlmClient, OpenAiCompatClient};

#[tokio::test]
async fn it_sends_messages_and_returns_assistant_content() {
    let server = MockServer::start().await;
    // Assert Bearer auth + POST to /chat/completions, expect exactly one
    // call (the .expect(1) fires on Mock drop if the call never happens).
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(header("authorization", "Bearer sk-test"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "chatcmpl-abc",
            "object": "chat.completion",
            "model": "gpt-4o-mini",
            "choices": [{
                "index": 0,
                "message": { "role": "assistant", "content": "Hello yogurt." },
                "finish_reason": "stop"
            }]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = OpenAiCompatClient::new(server.uri(), "sk-test".into(), "gpt-4o-mini".into());
    let resp = client
        .complete(ChatRequest {
            messages: vec![ChatMessage::user("hi")],
            stream: false,
        })
        .await
        .expect("client call ok");
    assert_eq!(resp.content, "Hello yogurt.");
    assert_eq!(resp.model, "gpt-4o-mini");
}

#[tokio::test]
async fn it_surfaces_4xx_as_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({
            "error": { "message": "Invalid API key", "type": "auth_error" }
        })))
        .mount(&server)
        .await;

    let client = OpenAiCompatClient::new(server.uri(), "sk-bad".into(), "gpt-4o-mini".into());
    let err = client
        .complete(ChatRequest {
            messages: vec![ChatMessage::user("hi")],
            stream: false,
        })
        .await
        .expect_err("should fail");
    let msg = format!("{err:#}");
    // Either the HTTP status code or the upstream error body must surface
    // so the operator can act on it.
    assert!(
        msg.contains("401") || msg.contains("Invalid API key"),
        "expected status or upstream message in error, got: {msg}"
    );
}

#[tokio::test]
async fn it_strips_trailing_slash_from_base_url() {
    // Regression guard: a Settings UI paste like "https://api.minimaxi.chat/v1/"
    // must not produce "https://api.minimaxi.chat/v1//chat/completions" on the
    // wire. wiremock would 404 such a request — green test == single slash.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model": "m",
            "choices": [{
                "message": { "role": "assistant", "content": "ok" },
                "finish_reason": "stop"
            }]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let base_with_slash = format!("{}/", server.uri());
    let client = OpenAiCompatClient::new(base_with_slash, "sk-test".into(), "m".into());
    let resp = client
        .complete(ChatRequest {
            messages: vec![ChatMessage::user("hi")],
            stream: false,
        })
        .await
        .expect("should hit single-slash endpoint");
    assert_eq!(resp.content, "ok");
}
