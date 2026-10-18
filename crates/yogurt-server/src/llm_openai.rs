//! LLM client resolution for the enhance handler.
//!
//! There is deliberately no automatic fallback from an unreachable `http` provider to a
//! local CLI. Silently rerouting a meeting's content to an unvetted backend because of a
//! network hiccup is a change the user must opt into by picking the CLI as the active provider.

use std::sync::Arc;

pub use yogurt_llm::OpenAiCompatClient;

/// Shared by enhance and chat so they always use the same model.
///
/// Priority:
///   1. `state.llm_override`, test injection only.
///   2. `YOGURT_LLM_*` env vars.
///   3. Active provider row. An unreadable key or a CLI missing from `$PATH` is a hard `Err`.
///   4. `MockLlm` when nothing is configured, so first-run users still see the hero flow.
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

/// Developer override. `None` unless all of `YOGURT_LLM_BASE_URL`, `YOGURT_LLM_API_KEY`
/// and `YOGURT_LLM_MODEL` are set.
pub fn from_env() -> Option<OpenAiCompatClient> {
    let base_url = std::env::var("YOGURT_LLM_BASE_URL").ok()?;
    let api_key = std::env::var("YOGURT_LLM_API_KEY").ok()?;
    let model = std::env::var("YOGURT_LLM_MODEL").ok()?;
    Some(OpenAiCompatClient::new(base_url, api_key, model))
}

/// `Ok(None)` means no active provider, so the caller falls back to mock.
/// `Err` means the active provider is unusable (unreadable key, CLI not on `$PATH`) and the
/// caller must surface it (502) rather than fall back.
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
        let cli_model = (!provider.cli_model.is_empty()).then_some(provider.cli_model.clone());
        let client = yogurt_llm::CliClient::locate(program, cli_model).map_err(|e| {
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
    // The error names the provider, never the key.
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
                cli_model: String::new(),
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
                cli_model: String::new(),
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
        // `Arc<dyn LlmClient>` isn't `Debug`, so `unwrap` is unavailable.
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
        assert!(msg.to_lowercase().contains("key"), "actionable msg: {msg}");
    }

    /// An unrecognized program never resolves, whatever CLIs the machine has installed.
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

    /// Resolves when `claude` is on `$PATH` and errors otherwise, never `Ok(None)`.
    /// Both branches are asserted so it passes on CI and on a machine with `claude`.
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
