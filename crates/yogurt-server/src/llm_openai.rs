//! LLM client resolution for the enhance handler.
//!
//! The hardcoded ~50 LOC OpenAI-compat client originally lived here. Plan
//! 05-01 promoted it into the new `yogurt-llm` crate behind the
//! `LlmClient` trait. This module now re-exports the canonical adapter
//! plus two constructors:
//!
//! - [`from_env`] — developer override via `YOGURT_LLM_*` env vars
//!   (highest priority, checked first).
//! - `from_active_provider` (private) — the production path: active
//!   provider row from the `providers` table, branching on its `adapter`
//!   (LLM-4). An `http` row builds `OpenAiCompatClient` from its stored API
//!   key; a `cli` row `yogurt_llm::CliClient::locate`s the named program
//!   (`claude` | `cursor-agent`) and needs no key at all.
//!
//! [`resolve`] also wraps an `http`-adapter active provider's client in
//! [`CliFallbackClient`] (LLM-1) when a local agent CLI is on `$PATH`: a
//! connect-class failure reaching the configured provider's `base_url`
//! retries once through the CLI instead of failing the request. This
//! applies only when an `http` provider is actually configured -
//! deliberately not when none is (so `MockLlm` stays the deterministic,
//! free, no-subprocess path for first-run users and, just as importantly,
//! for tests - an earlier draft preferred the CLI over `MockLlm` whenever
//! nothing was configured, and on a machine with `claude` on `$PATH` that
//! made `cargo test --workspace` silently shell out to the real CLI and
//! spend real API credits instead of getting deterministic mock output)
//! and not when the active provider is already `cli`-adapter (LLM-4) -
//! wrapping a CLI client's own fallback in another CLI client is a no-op
//! at best, since its errors are never the `reqwest::Error` connect
//! failures the wrapper looks for.

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
///   3. Active provider row, branching on `adapter` (LLM-4):
///      - `http`: `OpenAiCompatClient` + stored key, wrapped in
///        [`CliFallbackClient`] when a local agent CLI is on `$PATH`
///        (LLM-1). A configured provider whose key can't be read is still
///        a hard `Err` — that's a config problem, not an
///        unreachable-endpoint problem, so the CLI fallback does not apply
///        to it.
///      - `cli`: the located `CliClient` directly, no fallback wrapper —
///        it IS what the user picked. Its program not being on `$PATH` is
///        the same hard-`Err` contract as a missing HTTP key.
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
        Some(ResolvedProvider::Http(primary)) => match yogurt_llm::CliClient::discover() {
            Some(cli) => Ok(Arc::new(CliFallbackClient {
                primary,
                cli: Arc::new(cli),
            })),
            None => Ok(primary),
        },
        // LLM-4: the user explicitly picked this CLI as their provider -
        // it IS the primary, not a fallback behind one, so no
        // `CliFallbackClient` wrapping.
        Some(ResolvedProvider::Cli(client)) => Ok(client),
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

/// The active provider row resolved into a working client, tagged by which
/// adapter it came from - `resolve` needs to know this to decide whether
/// [`CliFallbackClient`] (LLM-1) applies: wrapping an already-CLI client in
/// another CLI fallback would be pointless (its errors are never the
/// `reqwest::Error` connect failures the wrapper looks for) and would cost
/// an extra `CliClient::discover` on every single resolution.
enum ResolvedProvider {
    Http(Arc<dyn LlmClient>),
    Cli(Arc<dyn LlmClient>),
}

/// Production resolution path: build a client from the active provider row
/// (`providers` table), branching on its `adapter` (LLM-4).
///
/// Contract (the enhance handler maps each arm directly):
/// - `Ok(None)` — no active provider configured; caller falls back to mock.
/// - `Ok(Some(_))` — active provider ready to use.
/// - `Err(_)` — active provider exists but isn't usable right now (an
///   `http` provider whose key can't be read, or a `cli` provider whose
///   program isn't on `$PATH`); caller must surface this (502), never
///   silently fall back to mock.
async fn from_active_provider(
    db: &yogurt_db::Db,
    keys: std::sync::Arc<dyn yogurt_db::keys::ApiKeyStore>,
) -> anyhow::Result<Option<ResolvedProvider>> {
    let Some(provider) = yogurt_db::providers::active(db)? else {
        return Ok(None);
    };
    if provider.adapter == yogurt_db::providers::adapter::CLI {
        let program = yogurt_llm::CliProgram::parse(&provider.model).ok_or_else(|| {
            anyhow::anyhow!(
                "provider '{}' ({}) has an unrecognized CLI program '{}'",
                provider.name,
                provider.id,
                provider.model
            )
        })?;
        let client = yogurt_llm::CliClient::locate(program).map_err(|e| {
            anyhow::anyhow!(
                "provider '{}' ({}) is active but its CLI could not be found: {e} \
                 - install it and make sure it's on $PATH",
                provider.name,
                provider.id
            )
        })?;
        return Ok(Some(ResolvedProvider::Cli(Arc::new(client))));
    }
    let key = keys.get(&provider.id)?;
    // T-x3u-01: error names the provider only — never the key value.
    match key {
        Some(key) => Ok(Some(ResolvedProvider::Http(Arc::new(
            OpenAiCompatClient::new(provider.base_url, key, provider.model),
        )))),
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
                adapter: yogurt_db::providers::adapter::HTTP.to_string(),
            },
        )
        .unwrap();
        set_active(db, &id).unwrap();
        id
    }

    fn insert_active_cli_provider(db: &Db, model: &str) -> String {
        let id = insert(
            db,
            NewProvider {
                name: "Claude Code (local CLI)".to_string(),
                base_url: String::new(),
                model: model.to_string(),
                adapter: yogurt_db::providers::adapter::CLI.to_string(),
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
        // `ResolvedProvider` isn't `Debug`, so match explicitly rather than
        // `Result::unwrap` chains that would require it.
        let resolved = match from_active_provider(&db, keys).await {
            Ok(Some(r)) => r,
            other => panic!("expected Ok(Some(_)), got {}", other.is_ok()),
        };
        let ResolvedProvider::Http(client) = resolved else {
            panic!("expected an Http-adapter resolution");
        };
        assert_eq!(client.model_name(), "MiniMax-Text-01");
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

    // ── LLM-4: explicit CLI provider selection ─────────────────────────

    /// Deterministic regardless of whether `claude`/`cursor-agent` happen
    /// to be installed on the machine running this test - an unrecognized
    /// `model` value on a `cli`-adapter row can never resolve, on any
    /// machine, so this doesn't fall into the same trap that made an
    /// earlier draft of LLM-1 shell out to a real CLI during `cargo test`.
    #[tokio::test]
    async fn cli_provider_with_unrecognized_program_errors_naming_provider() {
        let (db, keys) = fixture();
        insert_active_cli_provider(&db, "not-a-real-cli-program");
        let err = match from_active_provider(&db, keys).await {
            Err(e) => e,
            Ok(_) => panic!("expected Err for an unrecognized CLI program"),
        };
        let msg = err.to_string();
        assert!(
            msg.contains("Claude Code (local CLI)"),
            "error should name the provider: {msg}"
        );
        assert!(
            msg.contains("not-a-real-cli-program"),
            "error should name the bad program value: {msg}"
        );
    }

    /// A `cli`-adapter row with a recognized program (`claude` |
    /// `cursor-agent`) either resolves `Ok(Some(ResolvedProvider::Cli(_)))`
    /// when that binary happens to be on this machine's `$PATH`, or a hard
    /// `Err` naming the provider when it isn't - never a silent `Ok(None)`
    /// mock fallback, matching the missing-HTTP-key contract. Both
    /// branches are asserted so the test is meaningful on a machine with
    /// `claude` installed (mine) and one without (CI).
    #[tokio::test]
    async fn cli_provider_with_recognized_program_resolves_or_errors_naming_provider() {
        let (db, keys) = fixture();
        insert_active_cli_provider(&db, "claude");
        match from_active_provider(&db, keys).await {
            Ok(Some(ResolvedProvider::Cli(client))) => {
                assert_eq!(client.model_name(), "cli:claude");
            }
            Ok(other) => panic!(
                "expected None or a Cli resolution, got Some(Http): {}",
                other.is_some()
            ),
            Err(e) => {
                let msg = e.to_string();
                assert!(
                    msg.contains("Claude Code (local CLI)"),
                    "error should name the provider: {msg}"
                );
            }
        }
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
