//! Plan 05-01 Task 1 acceptance tests for the non-streaming
//! `OpenAiCompatClient` (`LlmClient::complete`) against a wiremock
//! stand-in OpenAI server.

use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};
use yogurt_llm::{ChatMessage, ChatRequest, LlmClient, OpenAiCompatClient};

/// Matches only when no `authorization` header is present at all. Used to
/// assert local-runtime providers (Ollama, LM Studio) get no auth header
/// when the stored key is empty - wiremock has no built-in "header
/// absent" matcher.
struct NoAuthHeader;

impl wiremock::Match for NoAuthHeader {
    fn matches(&self, request: &Request) -> bool {
        request.headers.get("authorization").is_none()
    }
}

struct ReasoningSplit(bool);

impl wiremock::Match for ReasoningSplit {
    fn matches(&self, request: &Request) -> bool {
        serde_json::from_slice::<serde_json::Value>(&request.body)
            .ok()
            .and_then(|body| {
                body.get("reasoning_split")
                    .and_then(|value| value.as_bool())
            })
            == Some(self.0)
    }
}

/// Matches a body whose `thinking.type` equals the given value, or - for
/// `None` - a body with no `thinking` key at all.
struct Thinking(Option<&'static str>);

impl wiremock::Match for Thinking {
    fn matches(&self, request: &Request) -> bool {
        serde_json::from_slice::<serde_json::Value>(&request.body)
            .ok()
            .map(|body| {
                body.get("thinking")
                    .and_then(|t| t.get("type"))
                    .and_then(|v| v.as_str())
                    == self.0
            })
            .unwrap_or(false)
    }
}

#[tokio::test]
async fn it_sends_messages_and_returns_assistant_content() {
    let server = MockServer::start().await;
    // Assert Bearer auth + POST to /chat/completions, expect exactly one
    // call (the .expect(1) fires on Mock drop if the call never happens).
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(header("authorization", "Bearer sk-test"))
        .and(Thinking(None))
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
async fn minimax_requests_separate_reasoning() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(ReasoningSplit(true))
        .and(Thinking(Some("disabled")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model": "MiniMax-M3",
            "choices": [{
                "message": { "role": "assistant", "content": "## Final" }
            }]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = OpenAiCompatClient::new(server.uri(), "sk-test".into(), "MiniMax-M3".into());
    let response = client
        .complete(ChatRequest {
            messages: vec![ChatMessage::user("summarize")],
            stream: false,
        })
        .await
        .expect("client call ok");

    assert_eq!(response.content, "## Final");
}

/// Regression for the bug where reasoning models leak
/// `<think>…</think>` inline in `message.content` despite the wire
/// request carrying `reasoning_split: true`. The model-agnostic backstop
/// in `complete()` must scrub any leaked reasoning before returning.
#[tokio::test]
async fn complete_strips_think_block_when_provider_leaks_it() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model": "MiniMax-M3",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "<think>\nlet me think…\n</think>\n\nThey agreed on 25% Monday."
                }
            }]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = OpenAiCompatClient::new(server.uri(), "sk-test".into(), "MiniMax-M3".into());
    let resp = client
        .complete(ChatRequest {
            messages: vec![ChatMessage::user("summarize")],
            stream: false,
        })
        .await
        .expect("client call ok");
    assert_eq!(
        resp.content, "They agreed on 25% Monday.",
        "think block must be stripped"
    );
}

/// Unknown sibling fields such as `reasoning_content` are ignored and never
/// leak into the visible response.
#[tokio::test]
async fn complete_discards_separate_reasoning_content_field() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model": "deepseek-reasoner",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Visible answer.",
                    "reasoning_content": "Internal chain-of-thought that must NOT appear."
                }
            }]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client =
        OpenAiCompatClient::new(server.uri(), "sk-test".into(), "deepseek-reasoner".into());
    let resp = client
        .complete(ChatRequest {
            messages: vec![ChatMessage::user("hi")],
            stream: false,
        })
        .await
        .expect("client call ok");
    assert_eq!(resp.content, "Visible answer.");
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

#[tokio::test]
async fn list_models_returns_ids_from_provider() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/models"))
        .and(header("authorization", "Bearer sk-test"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "object": "list",
            "data": [
                { "id": "gpt-4o-mini", "object": "model" },
                { "id": "gpt-4o", "object": "model" },
                { "id": "o4-mini", "object": "model" },
            ]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = OpenAiCompatClient::new(server.uri(), "sk-test".into(), "gpt-4o-mini".into());
    let models = client.list_models().await.expect("list_models ok");
    assert_eq!(
        models,
        vec![
            "gpt-4o-mini".to_string(),
            "gpt-4o".to_string(),
            "o4-mini".to_string()
        ]
    );
}

#[tokio::test]
async fn list_models_propagates_auth_failure() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/models"))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({
            "error": { "message": "Incorrect API key provided", "type": "auth_error" }
        })))
        .mount(&server)
        .await;

    let client = OpenAiCompatClient::new(server.uri(), "sk-bad".into(), "gpt-4o-mini".into());
    let err = client.list_models().await.expect_err("should fail");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("401") || msg.contains("Incorrect API key"),
        "expected status or upstream message in error, got: {msg}"
    );
}

#[tokio::test]
async fn list_models_strips_models_prefix() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "object": "list",
            "data": [
                { "id": "models/gemini-2.5-flash", "object": "model" },
                { "id": "gpt-4o", "object": "model" },
            ]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = OpenAiCompatClient::new(server.uri(), "sk-test".into(), "gemini-2.5-flash".into());
    let models = client.list_models().await.expect("list_models ok");
    assert_eq!(
        models,
        vec!["gemini-2.5-flash".to_string(), "gpt-4o".to_string()]
    );
}

#[tokio::test]
async fn list_models_sends_no_auth_header_when_key_is_empty() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/models"))
        .and(NoAuthHeader)
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "object": "list",
            "data": [{ "id": "llama3.2", "object": "model" }]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = OpenAiCompatClient::new(server.uri(), "".into(), "llama3.2".into());
    let models = client.list_models().await.expect("list_models ok");
    assert_eq!(models, vec!["llama3.2".to_string()]);
}

#[tokio::test]
async fn errors_never_contain_the_raw_key() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({
            "error": { "message": "Incorrect API key provided: sk-secret-123456" }
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/models"))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({
            "error": { "message": "Incorrect API key provided: sk-secret-123456" }
        })))
        .mount(&server)
        .await;

    let client = OpenAiCompatClient::new(
        server.uri(),
        "sk-secret-123456".into(),
        "gpt-4o-mini".into(),
    );

    let complete_err = client
        .complete(ChatRequest {
            messages: vec![ChatMessage::user("hi")],
            stream: false,
        })
        .await
        .expect_err("should fail");
    let complete_msg = format!("{complete_err:#}");
    assert!(
        !complete_msg.contains("sk-secret-123456"),
        "raw key leaked from complete(): {complete_msg}"
    );
    assert!(
        complete_msg.contains("[key redacted]"),
        "expected redaction marker in: {complete_msg}"
    );

    let list_err = client.list_models().await.expect_err("should fail");
    let list_msg = format!("{list_err:#}");
    assert!(
        !list_msg.contains("sk-secret-123456"),
        "raw key leaked from list_models(): {list_msg}"
    );
    assert!(
        list_msg.contains("[key redacted]"),
        "expected redaction marker in: {list_msg}"
    );
}
