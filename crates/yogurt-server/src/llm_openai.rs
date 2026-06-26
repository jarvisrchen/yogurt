//! Minimal hardcoded OpenAI-compatible chat completion client.
//! TACTICAL Phase 4 only (CONTEXT D-19/D-21); Phase 5 promotes to a
//! trait-bounded `LlmClient` with Keychain-backed credentials.

use anyhow::{anyhow, Context, Result};
use serde_json::json;

/// Non-streaming chat-completion client for any `/chat/completions`-shaped
/// HTTP endpoint (OpenAI, MiniMax, Ollama, vLLM, llama.cpp server, Groq,
/// Together, Fireworks, …). `#[allow(dead_code)]` until the enhance
/// handler in Plan 04-03 wires it up.
#[allow(dead_code)]
pub struct OpenAiCompatClient {
    base_url: String,
    api_key: String,
    model: String,
    http: reqwest::Client,
}

#[allow(dead_code)]
impl OpenAiCompatClient {
    /// Construct from `YOGURT_LLM_BASE_URL` + `YOGURT_LLM_API_KEY` +
    /// `YOGURT_LLM_MODEL`. Returns `None` if any are absent — callers fall
    /// back to `MockLlm`.
    pub fn from_env() -> Option<Self> {
        let base_url = std::env::var("YOGURT_LLM_BASE_URL").ok()?;
        let api_key = std::env::var("YOGURT_LLM_API_KEY").ok()?;
        let model = std::env::var("YOGURT_LLM_MODEL").ok()?;
        Some(Self::new(base_url, api_key, model))
    }

    /// Direct construction (test helper + Phase 5 transition target).
    pub fn new(base_url: String, api_key: String, model: String) -> Self {
        Self {
            base_url,
            api_key,
            model,
            http: reqwest::Client::new(),
        }
    }

    /// POST to `<base_url>/chat/completions` and return
    /// `choices[0].message.content`. Phase 5 adds SSE streaming.
    pub async fn complete(&self, system: &str, user: &str) -> Result<String> {
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let body = json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user},
            ],
            "stream": false,
        });
        let resp = self
            .http
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .context("llm http")?;
        let status = resp.status();
        let json: serde_json::Value = resp.json().await.context("llm parse")?;
        if !status.is_success() {
            return Err(anyhow!("llm {status}: {json}"));
        }
        json["choices"][0]["message"]["content"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow!("missing choices[0].message.content in: {json}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Spin up a tiny single-shot tokio TCP listener that speaks the
    /// minimum HTTP/1.1 a `reqwest::post` needs, captures the request
    /// body, and replies with a canned OpenAI-shaped JSON response. We
    /// avoid `wiremock` to keep dev-deps lean.
    async fn run_mock_chat_server(addr: std::net::SocketAddr) -> tokio::task::JoinHandle<String> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 8192];
            let n = sock.read(&mut buf).await.unwrap();
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            let body = serde_json::json!({
                "choices": [{
                    "message": {"role": "assistant", "content": "ok-from-mock"},
                    "finish_reason": "stop"
                }]
            })
            .to_string();
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            sock.write_all(resp.as_bytes()).await.unwrap();
            sock.shutdown().await.ok();
            req
        })
    }

    #[tokio::test]
    async fn it_posts_chat_completions_and_returns_content() {
        let addr: std::net::SocketAddr = "127.0.0.1:0".parse().unwrap();
        // Bind first to claim a free port, then hand the bound listener
        // off via a small dance: bind, get addr, drop, re-bind in helper.
        // Simpler: bind in helper directly with port 0 and read addr.
        let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
        let bound = listener.local_addr().unwrap();
        drop(listener);

        let handle = run_mock_chat_server(bound).await;
        let client = OpenAiCompatClient::new(
            format!("http://{bound}"),
            "test-key".into(),
            "test-model".into(),
        );
        let out = client.complete("you are a test", "say ok").await.unwrap();
        assert_eq!(out, "ok-from-mock");

        let req = handle.await.unwrap();
        assert!(
            req.contains("POST /chat/completions"),
            "expected POST to /chat/completions, got: {req}"
        );
        // reqwest sends headers lowercase. Case-insensitive check.
        let req_lower = req.to_lowercase();
        assert!(
            req_lower.contains("authorization: bearer test-key"),
            "expected Bearer auth header, got: {req}"
        );
        assert!(req.contains("test-model"), "expected model in body");
        assert!(req.contains("you are a test"), "expected system message");
        assert!(req.contains("say ok"), "expected user message");
    }

    #[test]
    fn from_env_returns_none_when_any_var_missing() {
        // Save + clear to avoid polluting other tests.
        let saved = (
            std::env::var("YOGURT_LLM_BASE_URL").ok(),
            std::env::var("YOGURT_LLM_API_KEY").ok(),
            std::env::var("YOGURT_LLM_MODEL").ok(),
        );
        // SAFETY: tests in the same process race on env vars. We mitigate by
        // running this synchronous test with no async tasks holding env
        // references; the assertion is solely about the absence path.
        unsafe {
            std::env::remove_var("YOGURT_LLM_BASE_URL");
            std::env::remove_var("YOGURT_LLM_API_KEY");
            std::env::remove_var("YOGURT_LLM_MODEL");
        }
        assert!(OpenAiCompatClient::from_env().is_none());

        // Restore.
        unsafe {
            if let Some(v) = saved.0 {
                std::env::set_var("YOGURT_LLM_BASE_URL", v);
            }
            if let Some(v) = saved.1 {
                std::env::set_var("YOGURT_LLM_API_KEY", v);
            }
            if let Some(v) = saved.2 {
                std::env::set_var("YOGURT_LLM_MODEL", v);
            }
        }
    }
}
