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
//! There is deliberately no automatic fallback from an unreachable `http`
//! provider to a local CLI. An earlier draft of this module did exactly
//! that (LLM-1) - wrap the active provider in a combinator that retried
//! through whichever CLI happened to be on `$PATH` on a connect-class
//! failure - and it was reverted: silently rerouting a meeting's real
//! content to a different, unvetted backend because of a network hiccup is
//! a behavior change a user should opt into, not one that happens to them.
//! The CLI is reachable only by explicitly picking it as the active
//! provider (LLM-4).

use std::sync::Arc;
use std::time::Duration;

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
///      - `http`: `OpenAiCompatClient` + stored key. A key that can't be
///        read is a hard `Err`.
///      - `cli`: the located `CliClient` directly. Its program not being
///        on `$PATH` is the same hard-`Err` contract as a missing key —
///        never a silent fallback to anything else.
///   4. `MockLlm` when nothing is configured at all (logged as a warn) so
///      first-run users still see the hero flow work.
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
        Some(client) => Ok(client),
        None => {
            tracing::warn!("no LLM provider configured; falling back to MockLlm");
            Ok(Arc::new(crate::llm_mock::MockLlm))
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
) -> anyhow::Result<Option<Arc<dyn yogurt_llm::LlmClient>>> {
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
        return Ok(Some(Arc::new(client)));
    }
    let key = keys.get(&provider.id)?;
    // T-x3u-01: error names the provider only — never the key value.
    match key {
        Some(key) => Ok(Some(Arc::new(OpenAiCompatClient::new(
            provider.base_url,
            key,
            provider.model,
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
        // `Arc<dyn LlmClient>` isn't `Debug`, so match explicitly rather
        // than `Result::unwrap` chains that would require it.
        let client = match from_active_provider(&db, keys).await {
            Ok(Some(c)) => c,
            other => panic!("expected Ok(Some(_)), got {}", other.is_ok()),
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
    /// machine.
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
    /// `cursor-agent`) either resolves `Ok(Some(_))` when that binary
    /// happens to be on this machine's `$PATH`, or a hard `Err` naming the
    /// provider when it isn't - never a silent `Ok(None)` mock fallback,
    /// matching the missing-HTTP-key contract. Both branches are asserted
    /// so the test is meaningful on a machine with `claude` installed
    /// (mine) and one without (CI).
    #[tokio::test]
    async fn cli_provider_with_recognized_program_resolves_or_errors_naming_provider() {
        let (db, keys) = fixture();
        insert_active_cli_provider(&db, "claude");
        match from_active_provider(&db, keys).await {
            Ok(Some(client)) => {
                assert_eq!(client.model_name(), "cli:claude");
            }
            Ok(None) => panic!("expected Some(_) or an Err, got None"),
            Err(e) => {
                let msg = e.to_string();
                assert!(
                    msg.contains("Claude Code (local CLI)"),
                    "error should name the provider: {msg}"
                );
            }
        }
    }
}
