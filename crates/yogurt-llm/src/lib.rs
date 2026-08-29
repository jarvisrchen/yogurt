//! Yogurt LLM client crate (Phase 5, Plan 05-01).
//!
//! Promotes Phase 4's hardcoded `OpenAiCompatClient` (`yogurt-server/src/
//! llm_openai.rs`) behind the `LlmClient` trait. No axum / no web deps —
//! Phase 6's chat handler consumes `stream()` directly from this crate
//! without going through the HTTP layer.
//!
//! ## Design (CONTEXT D-01..D-03)
//!
//! - **D-01:** `LlmClient` is dyn-compatible. Two methods: `complete` for
//!   one-shot JSON round-trips and `stream` for SSE. Phase 8 will implement
//!   the same trait against `whisper-rs`-shaped local models.
//! - **D-02:** `OpenAiCompatClient` is the only impl shipped in this plan.
//!   `base_url` is supplied by the caller (Phase 5 Settings UI reads it
//!   from the `providers` SQLite table) — there is intentionally no
//!   hardcoded `api.openai.com` / `api.minimaxi.chat` host anywhere in
//!   this crate or its callers.
//! - **D-03:** Tested via `wiremock` against an in-memory HTTP server
//!   (see `tests/mock_server.rs` and `tests/streaming.rs`).

mod streaming;
mod types;

pub use types::{ChatChunk, ChatMessage, ChatRequest, ChatResponse};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures_util::stream::BoxStream;
use std::time::Duration;

/// BL-5 (Phase 4 carry-over): hard upper bound on a single LLM HTTP
/// round-trip. A wedged provider (OpenAI 500 loop, Minimax cold-start,
/// Ollama tying up all CPU) would otherwise hang the enhance handler
/// indefinitely. 60s is generous for chat completion at o(1k–2k) tokens
/// and aggressive enough that the user notices a stall before refreshing.
const HTTP_TIMEOUT: Duration = Duration::from_secs(60);

/// Lower bound on connect (DNS + TCP + TLS) for `OpenAiCompatClient`.
/// Surfaces a clear error when the caller pasted a typo'd base_url
/// instead of stalling for the full `HTTP_TIMEOUT`.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Dyn-compatible LLM-client surface. Both methods consume a `ChatRequest`
/// by value so callers can build the request once and choose streaming vs.
/// non-streaming based on the caller's UI affordances.
#[async_trait]
pub trait LlmClient: Send + Sync {
    /// One-shot non-streaming completion. Implementations MUST set
    /// `stream=false` on the wire regardless of `req.stream`.
    async fn complete(&self, req: ChatRequest) -> Result<ChatResponse>;

    /// Streaming completion. Implementations MUST set `stream=true` on
    /// the wire regardless of `req.stream`. The returned stream is
    /// `Send + 'static` so the caller can spawn it onto a tokio task.
    async fn stream(&self, req: ChatRequest) -> Result<BoxStream<'static, Result<ChatChunk>>>;

    /// The model name this client sends on the wire, for provenance
    /// stamps (`meetings.llm_model`). Mocks return a fixed marker.
    fn model_name(&self) -> String;
}

/// OpenAI-compatible HTTP adapter. Speaks the `/chat/completions` shape —
/// works against OpenAI proper, Minimax (`api.minimaxi.chat/v1`), Ollama
/// (`localhost:11434/v1`), LM Studio (`localhost:1234/v1`), vLLM,
/// llama.cpp server, OpenRouter, Groq, Together, Fireworks.
///
/// Fields are intentionally private; `streaming.rs` reaches them through
/// the four `_for_streaming` accessors below to avoid leaking the internal
/// layout while still allowing module-local composition.
#[derive(Clone)]
pub struct OpenAiCompatClient {
    base_url: String,
    api_key: String,
    model: String,
    http: reqwest::Client,
}

impl OpenAiCompatClient {
    /// Construct from raw `base_url` (e.g. `https://api.minimaxi.chat/v1`)
    /// + `api_key` (Bearer token) + `model` (provider-specific name).
    ///
    /// Trailing `/` on `base_url` is stripped so `format!("{base_url}/
    /// chat/completions")` always produces exactly one separator.
    ///
    /// Reqwest client build "should never fail" with default config — we
    /// fall back to `Client::new()` on the impossible-in-practice error
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

    // ── Internal accessors for the sibling `streaming` module ──────────────
    // Marked `#[doc(hidden)]` so they stay off the public docs surface
    // while remaining `pub` (necessary because `streaming.rs` is in a
    // sibling module, not a child; making fields `pub(crate)` would also
    // work but exposes them to any other module added later).

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

    /// Probe the provider's `/models` endpoint and return the list of model
    /// ids it advertises. Used by the Settings page's `Refresh` button to
    /// populate the MODEL datalist with the live, authoritative list
    /// (presets ship a static hint; this overrides it once a key is on
    /// file). Errors are surfaced as-is - the UI shows them inline.
    ///
    /// Auth is only attached when `api_key` is non-empty: local runtimes
    /// (Ollama, LM Studio) don't need one and would otherwise get a
    /// `Bearer ` header with an empty token.
    ///
    /// Google's OpenAI-compat shim returns ids like `models/gemini-2.5-flash`;
    /// the leading `models/` is stripped so ids match the plain form used
    /// everywhere else (presets, the saved `model` field).
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

    /// Replace every occurrence of this client's API key in `text` with
    /// `[key redacted]`. Providers routinely quote the offending key back
    /// in error bodies ("Incorrect API key provided: sk-abc..."); this
    /// keeps that from smuggling the raw key into an HTTP response.
    ///
    /// Keys under 4 chars are skipped - too short to be a real secret, and
    /// substring-replacing them would shred unrelated error text.
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
        };
        let resp = self
            .http
            .post(self.endpoint())
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            // Drain the body for the error message - providers' error
            // payloads usually contain the actionable detail (rate limit
            // window, invalid model name, account suspended).
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
        Ok(ChatResponse { content, model })
    }

    async fn stream(&self, req: ChatRequest) -> Result<BoxStream<'static, Result<ChatChunk>>> {
        streaming::stream(self, req).await
    }
}
