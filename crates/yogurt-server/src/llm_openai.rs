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
