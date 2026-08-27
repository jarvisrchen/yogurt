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
//!   from the `providers` table + its API key from the Keychain-backed
//!   [`yogurt_db::keychain::ApiKeyStore`].

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
///   3. Active provider row + Keychain key. A configured provider whose
///      key can't be read is a hard `Err` — never a silent mock fallback.
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
        Some(c) => Ok(std::sync::Arc::new(c)),
        None => {
            tracing::warn!("no LLM provider configured; falling back to MockLlm");
            Ok(std::sync::Arc::new(crate::llm_mock::MockLlm))
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
/// (`providers` table) + its Keychain-stored API key.
///
/// Contract (the enhance handler maps each arm directly):
/// - `Ok(None)` — no active provider configured; caller falls back to mock.
/// - `Ok(Some(client))` — active provider with a readable key.
/// - `Err(_)` — active provider exists but its key could not be read;
///   caller must surface this (502), never silently fall back to mock.
pub async fn from_active_provider(
    db: &yogurt_db::Db,
    keys: std::sync::Arc<dyn yogurt_db::keychain::ApiKeyStore>,
) -> anyhow::Result<Option<OpenAiCompatClient>> {
    let Some(provider) = yogurt_db::providers::active(db)? else {
        return Ok(None);
    };
    // SET-10: Keychain reads can block for seconds (user prompt on first
    // access) — never call ApiKeyStore::get on the tokio reactor. Bounded
    // at 10s: a wedged Keychain (unanswered macOS access prompt) must fail
    // the request with an actionable message, not hang chat/enhance forever.
    let provider_id = provider.id.clone();
    let key = match tokio::time::timeout(
        Duration::from_secs(10),
        tokio::task::spawn_blocking(move || keys.get(&provider_id)),
    )
    .await
    {
        Ok(joined) => joined
            .map_err(|e| anyhow::anyhow!("key store task failed: {e}"))?
            .unwrap_or(None),
        Err(_) => anyhow::bail!(
            "macOS Keychain did not respond within 10s while reading the key for \
             provider '{}' - approve the Keychain access prompt if one is showing, \
             or re-enter the key in Settings",
            provider.name
        ),
    };
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
    use yogurt_db::keychain::{ApiKeyStore, MemoryKeyStore};
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
}
