//! LLM client resolution for the enhance handler.
//!
//! The hardcoded ~50 LOC OpenAI-compat client originally lived here. Plan
//! 05-01 promoted it into the new `yogurt-llm` crate behind the
//! `LlmClient` trait. This module now re-exports the canonical adapter
//! plus two constructors:
//!
//! - [`from_env`] — developer override via `YOGURT_LLM_*` env vars
//!   (highest priority, checked first).
//! - [`from_active_provider`] — the production path: active provider row
//!   from the `providers` table + its API key from the file-backed
//!   [`yogurt_db::keys::ApiKeyStore`].
//!
//! [`resolve`] also wraps an active provider's client in
//! [`CliFallbackClient`] (LLM-1) when a local agent CLI is on `$PATH`: a
//! connect-class failure reaching the configured provider's `base_url`
//! retries once through the CLI instead of failing the request. This
//! applies only when a provider is actually configured - deliberately not
//! when none is, so `MockLlm` stays the deterministic, free, no-subprocess
//! path for first-run users and (just as importantly) for tests. An
//! earlier draft preferred the CLI over `MockLlm` whenever nothing was
//! configured; on a machine with `claude` on `$PATH` that made
//! `cargo test --workspace` silently shell out to the real CLI and spend
//! real API credits instead of getting deterministic mock output.

use std::sync::Arc;
use std::time::Duration;

use futures_util::stream::BoxStream;
use yogurt_llm::{ChatChunk, ChatRequest, ChatResponse, LlmClient};

// Re-export the canonical adapter from `yogurt-llm`.
pub use yogurt_llm::OpenAiCompatClient;

/// BL-5 (Phase 4 carry-over): hard ceiling on a single LLM HTTP
/// round-trip. Re-exported as a `const Duration` so the enhance handler
/// can wrap calls in `tokio::time::timeout(LLM_HTTP_TIMEOUT, …)`. The
/// `yogurt-llm` adapter itself also configures `reqwest::Client::timeout`
/// to the same value — defense-in-depth.
pub const LLM_HTTP_TIMEOUT: Duration = Duration::from_secs(60);

/// Resolve the LLM client for a request. Single source of truth shared by
/// the enhance handler AND the chat handler so they can never disagree
/// about which model answers (the original Phase 6 bug: chat pinned to
/// `MockLlm` while enhance resolved the real provider).
///
/// Priority chain:
///   1. `state.llm_override` — test injection, never set in production.
///   2. `YOGURT_LLM_*` env vars — developer override.
///   3. Active provider row + stored key, wrapped in [`CliFallbackClient`]
///      when a local agent CLI is on `$PATH` (LLM-1). A configured
///      provider whose key can't be read is still a hard `Err` — that's a
///      config problem, not an unreachable-endpoint problem, so the CLI
///      fallback does not apply to it.
///   4. `MockLlm` when nothing is configured at all (logged as a warn) so
///      first-run users still see the hero flow work. Deliberately not the
///      CLI fallback here — see the module doc.
pub async fn resolve(
    state: &crate::state::AppState,
) -> anyhow::Result<std::sync::Arc<dyn yogurt_llm::LlmClient>> {
    if let Some(overridden) = &state.llm_override {
        return Ok(overridden.clone());
    }
    if let Some(c) = from_env() {
        return Ok(std::sync::Arc::new(c));
    }
    match from_active_provider(&state.db, state.keys.clone()).await? {
        Some(c) => {
            let primary: Arc<dyn LlmClient> = Arc::new(c);
            match yogurt_llm::CliClient::discover() {
                Some(cli) => Ok(Arc::new(CliFallbackClient {
                    primary,
                    cli: Arc::new(cli),
                })),
                None => Ok(primary),
            }
        }
        None => {
            tracing::warn!("no LLM provider configured; falling back to MockLlm");
            Ok(Arc::new(crate::llm_mock::MockLlm))
        }
    }
}

/// Wraps the configured provider client with the local agent CLI as a
/// fallback (LLM-1): when the configured `base_url` can't be reached at
/// all — corporate egress blocking it is the motivating case, not an auth
/// or rate-limit error — retry once through whichever CLI
/// [`yogurt_llm::CliClient::discover`] found on `$PATH`.
///
/// Only a *connect*-class failure triggers the fallback. Anything else
/// (401, 429, a malformed response) is a configuration problem the CLI
/// can't fix, so it surfaces exactly as it does today.
///
/// `model_name()` always reports the configured provider's model, even on
/// a call that actually fell back — `meetings.llm_model` is already a
/// best-effort cost-attribution stamp (see `ChatResponse::model`'s doc
/// comment on providers renaming themselves in responses), and the
/// `tracing::warn!` at the fallback site is the source of truth for what
/// actually answered. Threading the real model back into that stamp would
/// mean querying `model_name()` only after a stream opens instead of
/// before, in both `enhance.rs` and `chat.rs` — not done here (ponytail:
/// deferred; revisit if provenance-on-fallback turns out to matter).
struct CliFallbackClient {
    primary: Arc<dyn LlmClient>,
    cli: Arc<dyn LlmClient>,
}

impl CliFallbackClient {
    fn is_connect_failure(err: &anyhow::Error) -> bool {
        err.downcast_ref::<reqwest::Error>()
            .map(|e| e.is_connect() || e.is_timeout())
            .unwrap_or(false)
    }

    fn log_fallback(&self, error: &anyhow::Error) {
        tracing::warn!(
            event = "llm_cli_fallback_used",
            primary_model = %self.primary.model_name(),
            cli_model = %self.cli.model_name(),
            error = %error,
            "primary LLM provider unreachable, falling back to local agent CLI"
        );
    }
}

#[async_trait::async_trait]
impl LlmClient for CliFallbackClient {
    fn model_name(&self) -> String {
        self.primary.model_name()
    }

    async fn complete(&self, req: ChatRequest) -> anyhow::Result<ChatResponse> {
        match self.primary.complete(req.clone()).await {
            Ok(resp) => Ok(resp),
            Err(e) if Self::is_connect_failure(&e) => {
                self.log_fallback(&e);
                self.cli.complete(req).await
            }
            Err(e) => Err(e),
        }
    }

    async fn stream(
        &self,
        req: ChatRequest,
    ) -> anyhow::Result<BoxStream<'static, anyhow::Result<ChatChunk>>> {
        match self.primary.stream(req.clone()).await {
            Ok(s) => Ok(s),
            Err(e) if Self::is_connect_failure(&e) => {
                self.log_fallback(&e);
                self.cli.stream(req).await
            }
            Err(e) => Err(e),
        }
    }
}

/// `.env`-based developer override. Returns `None` if any of the three
/// `YOGURT_LLM_*` env vars is missing; the enhance handler then falls
/// through to [`from_active_provider`] (and finally `MockLlm`). When set,
/// `YOGURT_LLM_BASE_URL` + `YOGURT_LLM_API_KEY` + `YOGURT_LLM_MODEL` win
/// over the configured provider row.
pub fn from_env() -> Option<OpenAiCompatClient> {
    let base_url = std::env::var("YOGURT_LLM_BASE_URL").ok()?;
    let api_key = std::env::var("YOGURT_LLM_API_KEY").ok()?;
    let model = std::env::var("YOGURT_LLM_MODEL").ok()?;
    Some(OpenAiCompatClient::new(base_url, api_key, model))
}

/// Production resolution path: build a client from the active provider row
/// (`providers` table) + its stored API key.
///
/// Contract (the enhance handler maps each arm directly):
/// - `Ok(None)` — no active provider configured; caller falls back to mock.
/// - `Ok(Some(client))` — active provider with a readable key.
/// - `Err(_)` — active provider exists but its key could not be read;
///   caller must surface this (502), never silently fall back to mock.
pub async fn from_active_provider(
    db: &yogurt_db::Db,
    keys: std::sync::Arc<dyn yogurt_db::keys::ApiKeyStore>,
) -> anyhow::Result<Option<OpenAiCompatClient>> {
    let Some(provider) = yogurt_db::providers::active(db)? else {
        return Ok(None);
    };
    let key = keys.get(&provider.id)?;
    // T-x3u-01: error names the provider only — never the key value.
    match key {
        Some(key) => Ok(Some(OpenAiCompatClient::new(
            provider.base_url,
            key,
            provider.model,
        ))),
        None => anyhow::bail!(
            "provider '{}' ({}) is active but its API key could not be read \
             from the key store - re-enter it in Settings",
            provider.name,
            provider.id
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use yogurt_db::keys::{ApiKeyStore, MemoryKeyStore};
    use yogurt_db::providers::{insert, set_active, NewProvider};
    use yogurt_db::Db;

    fn fixture() -> (Db, Arc<dyn ApiKeyStore>) {
        let db = Db::open_in_memory().unwrap();
        let keys: Arc<dyn ApiKeyStore> = Arc::new(MemoryKeyStore::default());
        (db, keys)
    }

    fn insert_active_provider(db: &Db) -> String {
        let id = insert(
            db,
            NewProvider {
                name: "Minimax".to_string(),
                base_url: "https://api.minimax.io/v1".to_string(),
                model: "MiniMax-Text-01".to_string(),
            },
        )
        .unwrap();
        set_active(db, &id).unwrap();
        id
    }

    #[tokio::test]
    async fn no_provider_returns_none() {
        let (db, keys) = fixture();
        let got = from_active_provider(&db, keys).await.unwrap();
        assert!(got.is_none());
    }

    #[tokio::test]
    async fn provider_with_key_returns_client() {
        let (db, keys) = fixture();
        let id = insert_active_provider(&db);
        keys.set(&id, "sk-test").unwrap();
        let client = from_active_provider(&db, keys).await.unwrap().unwrap();
        assert_eq!(client.base_url_for_streaming(), "https://api.minimax.io/v1");
        assert_eq!(client.model_for_streaming(), "MiniMax-Text-01");
    }

    #[tokio::test]
    async fn provider_without_key_errors_naming_provider() {
        let (db, keys) = fixture();
        insert_active_provider(&db);
        let err = match from_active_provider(&db, keys).await {
            Err(e) => e,
            Ok(_) => panic!("expected Err when key is missing"),
        };
        let msg = err.to_string();
        assert!(msg.contains("Minimax"), "error should name provider: {msg}");
        // T-x3u-01: message must never contain a key value; here just assert
        // it points the user at re-entering the key.
        assert!(msg.to_lowercase().contains("key"), "actionable msg: {msg}");
    }

    // ── CliFallbackClient ───────────────────────────────────────────────

    use futures_util::stream::StreamExt;
    use std::sync::atomic::{AtomicUsize, Ordering};

    enum FakeOutcome {
        Ok(&'static str),
        ConnectErr,
        OtherErr,
    }

    struct FakeClient {
        name: &'static str,
        outcome: FakeOutcome,
        calls: AtomicUsize,
    }

    #[async_trait::async_trait]
    impl LlmClient for FakeClient {
        fn model_name(&self) -> String {
            self.name.to_string()
        }

        async fn complete(&self, _req: ChatRequest) -> anyhow::Result<ChatResponse> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            match self.outcome {
                FakeOutcome::Ok(content) => Ok(ChatResponse {
                    content: content.to_string(),
                    model: self.name.to_string(),
                }),
                FakeOutcome::ConnectErr => Err(connect_refused_error().await),
                FakeOutcome::OtherErr => Err(anyhow::anyhow!("401 unauthorized")),
            }
        }

        async fn stream(
            &self,
            req: ChatRequest,
        ) -> anyhow::Result<BoxStream<'static, anyhow::Result<ChatChunk>>> {
            let resp = self.complete(req).await?;
            Ok(futures_util::stream::iter(vec![Ok(ChatChunk {
                delta: resp.content,
                done: true,
            })])
            .boxed())
        }
    }

    /// A real `reqwest::Error` with `is_connect() == true`: bind an
    /// ephemeral port, drop the listener so nothing answers on it, then
    /// try to connect. Deterministic connection-refused, no reliance on
    /// an unowned port number or DNS behavior.
    async fn connect_refused_error() -> anyhow::Error {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let err = reqwest::Client::new()
            .get(format!("http://127.0.0.1:{port}/"))
            .send()
            .await
            .expect_err("nothing should be listening on a just-dropped port");
        assert!(err.is_connect(), "expected a connect error, got: {err:?}");
        anyhow::Error::from(err)
    }

    #[tokio::test]
    async fn fallback_triggers_on_connect_failure() {
        let primary = Arc::new(FakeClient {
            name: "primary",
            outcome: FakeOutcome::ConnectErr,
            calls: AtomicUsize::new(0),
        });
        let cli = Arc::new(FakeClient {
            name: "cli",
            outcome: FakeOutcome::Ok("cli answer"),
            calls: AtomicUsize::new(0),
        });
        let wrapper = CliFallbackClient {
            primary: primary.clone(),
            cli: cli.clone(),
        };

        let resp = wrapper
            .complete(ChatRequest {
                messages: vec![],
                stream: false,
            })
            .await
            .unwrap();

        assert_eq!(resp.content, "cli answer");
        assert_eq!(cli.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn non_connect_failure_does_not_fall_back() {
        let primary = Arc::new(FakeClient {
            name: "primary",
            outcome: FakeOutcome::OtherErr,
            calls: AtomicUsize::new(0),
        });
        let cli = Arc::new(FakeClient {
            name: "cli",
            outcome: FakeOutcome::Ok("cli answer"),
            calls: AtomicUsize::new(0),
        });
        let wrapper = CliFallbackClient {
            primary: primary.clone(),
            cli: cli.clone(),
        };

        let err = wrapper
            .complete(ChatRequest {
                messages: vec![],
                stream: false,
            })
            .await
            .unwrap_err();

        assert!(err.to_string().contains("401"));
        assert_eq!(cli.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn model_name_reports_primary_even_after_a_fallback_call() {
        let primary = Arc::new(FakeClient {
            name: "primary-model",
            outcome: FakeOutcome::ConnectErr,
            calls: AtomicUsize::new(0),
        });
        let cli = Arc::new(FakeClient {
            name: "cli-model",
            outcome: FakeOutcome::Ok("cli answer"),
            calls: AtomicUsize::new(0),
        });
        let wrapper = CliFallbackClient { primary, cli };

        let _ = wrapper
            .complete(ChatRequest {
                messages: vec![],
                stream: false,
            })
            .await
            .unwrap();

        assert_eq!(wrapper.model_name(), "primary-model");
    }
}
