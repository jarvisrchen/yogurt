//! LLM client behind the dyn-compatible [`LlmClient`] trait. No axum or web deps.
//! `base_url` always comes from the caller, so no provider host is hardcoded here.

mod cli;
mod streaming;
mod thinking;
mod types;

pub use cli::{list_models as list_cli_models, CliClient, CliProgram};
pub use thinking::strip_thinking;
pub use types::{ChatChunk, ChatMessage, ChatRequest, ChatResponse};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures_util::stream::BoxStream;
use std::time::Duration;

/// Hard upper bound on a single LLM HTTP round-trip, so a wedged provider cannot hang the enhance handler.
const HTTP_TIMEOUT: Duration = Duration::from_secs(60);

/// Budget for a client whose `stream` does not actually stream and must finish generating before it yields (the agent CLIs in `cli.rs`).
///
/// Against an HTTP provider `stream()` resolves at the response headers, so `HTTP_TIMEOUT` only has to cover a handshake.
/// `CliClient::stream` runs the subprocess to completion first, so the budget has to cover full generation.
/// `claude --model haiku` ignores `--effort low` and can spend 90+ s thinking on a short answer.
/// A wedged process is still bounded by the adapter's own `CLI_TIMEOUT`.
const GENERATION_TIMEOUT: Duration = Duration::from_secs(300);

/// Connect (DNS + TCP + TLS) bound, so a typo'd base_url fails fast instead of stalling for `HTTP_TIMEOUT`.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

const CONNECT_RETRY_DELAY: Duration = Duration::from_secs(1);

/// A request that failed to connect never reached the provider, so one retry is safe and rides out a network blip.
async fn send(rb: reqwest::RequestBuilder) -> reqwest::Result<reqwest::Response> {
    let retry = rb.try_clone();
    match rb.send().await {
        Err(e) if e.is_connect() => match retry {
            Some(rb) => {
                tokio::time::sleep(CONNECT_RETRY_DELAY).await;
                rb.send().await
            }
            None => Err(e),
        },
        res => res,
    }
}

#[async_trait]
pub trait LlmClient: Send + Sync {
    /// Must set `stream=false` on the wire regardless of `req.stream`.
    async fn complete(&self, req: ChatRequest) -> Result<ChatResponse>;

    /// Must set `stream=true` on the wire regardless of `req.stream`.
    async fn stream(&self, req: ChatRequest) -> Result<BoxStream<'static, Result<ChatChunk>>>;

    /// The model name sent on the wire, stamped into `meetings.llm_model`.
    fn model_name(&self) -> String;

    /// How long a caller should wait before calling this client failed, both for `stream()` to resolve and between chunks.
    ///
    /// A trait method because only the implementation knows what each await point covers. See [`GENERATION_TIMEOUT`].
    fn response_timeout(&self) -> Duration {
        HTTP_TIMEOUT
    }
}

/// OpenAI-compatible HTTP adapter for the `/chat/completions` shape (OpenAI, MiniMax, Ollama, LM Studio, vLLM, OpenRouter, and similar).
#[derive(Clone)]
pub struct OpenAiCompatClient {
    base_url: String,
    api_key: String,
    model: String,
    http: reqwest::Client,
}

impl OpenAiCompatClient {
    /// Trailing `/` on `base_url` is stripped so the endpoint has exactly one separator.
    ///
    /// A failed client build falls back to `Client::new()` rather than panicking in a constructor.
    /// path rather than panic in a library constructor.
    pub fn new(base_url: String, api_key: String, model: String) -> Self {
        let http = reqwest::Client::builder()
            .timeout(HTTP_TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            model,
            http,
        }
    }

    fn endpoint(&self) -> String {
        format!("{}/chat/completions", self.base_url)
    }

    // `pub` because `streaming` is a sibling module, not a child.

    #[doc(hidden)]
    pub fn http_for_streaming(&self) -> &reqwest::Client {
        &self.http
    }

    #[doc(hidden)]
    pub fn base_url_for_streaming(&self) -> &str {
        &self.base_url
    }

    #[doc(hidden)]
    pub fn api_key_for_streaming(&self) -> &str {
        &self.api_key
    }

    #[doc(hidden)]
    pub fn model_for_streaming(&self) -> &str {
        &self.model
    }

    #[doc(hidden)]
    pub fn split_reasoning(&self) -> bool {
        self.model.to_ascii_lowercase().starts_with("minimax-")
    }

    /// MiniMax reasoning models spend an unpredictable 700-5700 hidden reasoning tokens per call, which pushes the tail past the HTTP timeout.
    /// Enhance and chat are extraction jobs, so thinking is switched off.
    /// `reasoning_effort` is ignored by their API; only `thinking.type = "disabled"` works.
    #[doc(hidden)]
    pub(crate) fn thinking(&self) -> Option<types::Thinking> {
        self.split_reasoning()
            .then_some(types::Thinking { r#type: "disabled" })
    }

    /// List the model ids the provider's `/models` endpoint advertises.
    ///
    /// Auth is attached only for a non-empty key: local runtimes (Ollama, LM Studio) need none.
    /// Google's compat shim returns ids like `models/gemini-2.5-flash`, so the `models/` prefix is stripped.
    pub async fn list_models(&self) -> Result<Vec<String>> {
        let url = format!("{}/models", self.base_url);
        let mut req = self.http.get(&url);
        if !self.api_key.is_empty() {
            req = req.bearer_auth(&self.api_key);
        }
        let resp = req.send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("LLM call failed: {status} - {}", self.scrub(&body)));
        }
        let parsed: types::ModelsResponse = resp.json().await?;
        Ok(parsed
            .data
            .into_iter()
            .map(|m| {
                m.id.strip_prefix("models/")
                    .map(str::to_string)
                    .unwrap_or(m.id)
            })
            .collect())
    }

    /// Replace this client's API key in `text` with `[key redacted]`, since providers quote the key back in error bodies.
    ///
    /// Keys under 4 chars are skipped: substring-replacing them would shred unrelated text.
    fn scrub(&self, text: &str) -> String {
        if self.api_key.len() < 4 {
            return text.to_string();
        }
        text.replace(&self.api_key, "[key redacted]")
    }
}

#[async_trait]
impl LlmClient for OpenAiCompatClient {
    fn model_name(&self) -> String {
        self.model.clone()
    }

    async fn complete(&self, req: ChatRequest) -> Result<ChatResponse> {
        let body = types::OpenAiRequest {
            model: &self.model,
            messages: &req.messages,
            stream: false,
            reasoning_split: self.split_reasoning(),
            thinking: self.thinking(),
        };
        let resp = send(
            self.http
                .post(self.endpoint())
                .bearer_auth(&self.api_key)
                .json(&body),
        )
        .await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("LLM call failed: {status} - {}", self.scrub(&body)));
        }
        let parsed: types::OpenAiResponse = resp.json().await?;
        let model = parsed.model;
        let content = parsed
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("no choices in response"))?
            .message
            .content;
        let content = crate::thinking::strip_thinking(&content);
        Ok(ChatResponse { content, model })
    }

    async fn stream(&self, req: ChatRequest) -> Result<BoxStream<'static, Result<ChatChunk>>> {
        streaming::stream(self, req).await
    }
}
