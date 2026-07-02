//! Phase 4 → Phase 5 (Plan 05-01) compat shim.
//!
//! The hardcoded ~50 LOC OpenAI-compat client originally lived here. Plan
//! 05-01 promoted it into the new `yogurt-llm` crate behind the
//! `LlmClient` trait. This module now re-exports the canonical adapter
//! and a small `from_env()` constructor (Phase 4-shaped fallback used by
//! the enhance handler until Plan 05-02's `AppState.keys` is wired) so
//! existing call-sites compile unchanged.

use std::time::Duration;

// Re-export the canonical adapter from `yogurt-llm`.
pub use yogurt_llm::OpenAiCompatClient;

/// BL-5 (Phase 4 carry-over): hard ceiling on a single LLM HTTP
/// round-trip. Re-exported as a `const Duration` so the enhance handler
/// can wrap calls in `tokio::time::timeout(LLM_HTTP_TIMEOUT, …)`. The
/// `yogurt-llm` adapter itself also configures `reqwest::Client::timeout`
/// to the same value — defense-in-depth.
pub const LLM_HTTP_TIMEOUT: Duration = Duration::from_secs(60);

/// Phase 4-shape `.env`-based constructor. Returns `None` if any of the
/// three `YOGURT_LLM_*` env vars is missing; the enhance handler falls
/// back to `MockLlm` in that case.
///
/// Plan 05-02 will replace this with a lookup against `AppState.db`
/// (active provider row) + `AppState.keys` (Keychain-backed
/// `ApiKeyStore`). Until that lands, this shim keeps the enhance handler
/// working against a real provider when the developer sets
/// `YOGURT_LLM_BASE_URL` + `YOGURT_LLM_API_KEY` + `YOGURT_LLM_MODEL` in
/// their environment.
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
    _db: &yogurt_db::Db,
    _keys: std::sync::Arc<dyn yogurt_db::keychain::ApiKeyStore>,
) -> anyhow::Result<Option<OpenAiCompatClient>> {
    todo!("GREEN step of quick-260701-x3u task 1")
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
