//! Seed providers from `YOGURT_*_API_KEY` env vars on first run.
//!
//! Idempotent: existing rows (matched case-insensitively by name) are
//! skipped, never overwritten. The first LLM provider seeded in a run
//! becomes active iff no other LLM provider was already active.
//!
//! Phase 5 (Plan 05-02), Task 3.
//!
//! ## Release-build invariant (SET-11)
//!
//! `seed_from_env` itself just reads `std::env`; the `.env.local` loader
//! that populates those env vars lives in `yogurt-cli/src/main.rs` and is
//! gated on the `--dev` CLI flag. Release builds invoked without `--dev`
//! see only the inherited environment — they DO NOT read `.env.local`.
//!
//! That layering means `seed_from_env` can run unconditionally at every
//! boot: if no `YOGURT_*_API_KEY` env vars are present (the brew-install
//! release case), it returns an empty `SeedReport` and is a no-op.

use crate::state::AppState;
use anyhow::Result;
use yogurt_db::providers::{self, NewProvider};

/// `(env_var, provider_name, base_url, default_model)`.
///
/// Only LLM-kind providers are listed here for v1. STT providers (Deepgram,
/// AssemblyAI, Groq) are deferred to Phase 8 when the `kind='stt'` surface
/// gets first-class support in the providers CRUD.
const ENV_PRESETS: &[(&str, &str, &str, &str)] = &[
    (
        "YOGURT_MINIMAX_API_KEY",
        "Minimax",
        "https://api.minimax.io/v1",
        "MiniMax-Text-01",
    ),
    (
        "YOGURT_OPENAI_API_KEY",
        "OpenAI",
        "https://api.openai.com/v1",
        "gpt-4o-mini",
    ),
    (
        "YOGURT_OPENROUTER_API_KEY",
        "OpenRouter",
        "https://openrouter.ai/api/v1",
        "anthropic/claude-3.5-sonnet",
    ),
];

/// Report of which env-var-backed providers were seeded vs already present.
#[derive(Debug, Default)]
pub struct SeedReport {
    /// Provider names newly inserted on this call (in iteration order).
    pub seeded: Vec<String>,
    /// Env-var-present names skipped because a same-named row already exists.
    pub skipped: Vec<String>,
}

/// Seed providers from `YOGURT_*_API_KEY` env vars.
///
/// For each preset in [`ENV_PRESETS`]:
/// 1. Skip if env var is absent or trims to empty.
/// 2. Skip (and record in `skipped`) if a provider with the same name
///    already exists (case-insensitive).
/// 3. Otherwise: insert the provider row, write the key to Keychain
///    (non-fatal: a Keychain write failure is logged but doesn't abort the
///    bootstrap — the user can re-enter the key via the Settings UI).
/// 4. If this is the first LLM provider seeded and no provider is currently
///    active, mark this one active.
pub async fn seed_from_env(state: &AppState) -> Result<SeedReport> {
    let mut report = SeedReport::default();
    let existing = providers::list(&state.db)?;

    for &(env_var, name, base_url, model) in ENV_PRESETS {
        let Ok(key) = std::env::var(env_var) else {
            continue;
        };
        if key.trim().is_empty() {
            continue;
        }

        // If a row with this name already exists, BACKFILL the Keychain
        // entry when missing instead of skipping silently. The previous
        // behavior skipped the entire iteration — so a user who:
        //   (1) booted with an empty .env.local stub (row never inserted)
        //   (2) clicked the Minimax preset chip in Settings to scaffold
        //       a row (row inserted, no key stored)
        //   (3) added the key to .env.local and re-ran `just dev`
        // would see the provider row but `No key stored yet.` in the UI,
        // with no log line explaining why. Backfill restores the intent.
        if let Some(existing_row) = existing
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(name))
        {
            let existing_key = state.keys.get(&existing_row.id).ok().flatten();
            if existing_key.is_none() {
                if let Err(e) = state.keys.set(&existing_row.id, &key) {
                    tracing::warn!(provider = name, error = %e, "failed to backfill key in keychain");
                    report.skipped.push(name.to_string());
                } else {
                    tracing::info!(provider = name, "backfilled missing keychain key from env");
                    report.seeded.push(name.to_string());
                }
            } else {
                report.skipped.push(name.to_string());
            }
            continue;
        }

        let id = providers::insert(
            &state.db,
            NewProvider {
                name: name.to_string(),
                base_url: base_url.to_string(),
                model: model.to_string(),
            },
        )?;

        // Store the key in the configured `ApiKeyStore` (Keychain in
        // production, MemoryKeyStore in tests). Non-fatal on failure —
        // the user can re-enter the key via the Settings UI.
        if let Err(e) = state.keys.set(&id, &key) {
            tracing::warn!(provider = name, error = %e, "failed to store key in keychain");
        }

        // First LLM seeded → active iff nothing else active. We treat
        // "first" by checking after each insert so the iteration-order
        // guarantee in ENV_PRESETS holds (Minimax first).
        if providers::active(&state.db)?.is_none() {
            providers::set_active(&state.db, &id)?;
        }

        report.seeded.push(name.to_string());
    }

    Ok(report)
}
