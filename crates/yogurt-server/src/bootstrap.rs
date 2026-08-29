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
        "YOGURT_GEMINI_API_KEY",
        "Google Gemini",
        "https://generativelanguage.googleapis.com/v1beta/openai",
        "gemini-2.5-flash",
    ),
    (
        "YOGURT_DEEPSEEK_API_KEY",
        "DeepSeek",
        "https://api.deepseek.com/v1",
        "deepseek-chat",
    ),
    (
        "YOGURT_OPENROUTER_API_KEY",
        "OpenRouter",
        "https://openrouter.ai/api/v1",
        "anthropic/claude-3.5-sonnet",
    ),
];

/// The env var names [`ENV_PRESETS`] reads, in order.
///
/// Exposed so `tests/bootstrap.rs` can clear ALL of them before asserting on
/// a single seeded provider. The tests used to hand-list the vars, which
/// meant adding a preset here silently broke them on any machine that had
/// the new key exported — the same stale-enumeration trap that bit the STT
/// model registry.
pub fn env_key_vars() -> impl Iterator<Item = &'static str> {
    ENV_PRESETS.iter().map(|&(var, ..)| var)
}

/// Report of which env-var-backed providers were seeded vs already present.
#[derive(Debug, Default)]
pub struct SeedReport {
    /// Provider names newly inserted on this call (in iteration order).
    pub seeded: Vec<String>,
    /// Env-var-present names skipped because a same-named row already exists.
    pub skipped: Vec<String>,
}

/// Seed the Deepgram STT key from `YOGURT_DEEPGRAM_API_KEY` into the key
/// store under [`crate::meetings::DEEPGRAM_KEY_ID`]. Idempotent: an
/// existing stored key is never overwritten (the Settings UI owns it once
/// set). Separate from the LLM provider presets because STT keys have no
/// `providers` table row — the store entry is the whole configuration.
pub fn seed_stt_key_from_env(state: &AppState) {
    let Ok(key) = std::env::var("YOGURT_DEEPGRAM_API_KEY") else {
        return;
    };
    if key.trim().is_empty() {
        return;
    }
    let existing = state
        .keys
        .get(crate::meetings::DEEPGRAM_KEY_ID)
        .ok()
        .flatten();
    if existing.is_some() {
        return;
    }
    match state.keys.set(crate::meetings::DEEPGRAM_KEY_ID, key.trim()) {
        Ok(()) => tracing::info!("seeded Deepgram STT key from env"),
        Err(e) => tracing::warn!(error = %e, "failed to seed Deepgram STT key"),
    }
}

/// One-time repair for `enriched_md` rows corrupted by the pre-260813
/// enhance bug: the model's marker spans were HTML-escaped and then wrapped
/// in real spans again, so the stored markdown contains literal
/// `&lt;span data-ai-grey …&gt;` text that renders as garbage. Idempotent
/// (a clean row contains no escaped span markers) and cheap (a LIKE scan
/// over a local table), so it runs on every boot.
pub fn repair_escaped_span_rows(state: &AppState) -> Result<usize> {
    let rows: Vec<(String, String)> = {
        let reader = state.storage.read();
        let conn = reader.lock().map_err(|e| anyhow::anyhow!("db lock: {e}"))?;
        let mut stmt = conn.prepare(
            "SELECT id, enriched_md FROM meetings \
             WHERE enriched_md LIKE '%&lt;span data-ai-grey%'",
        )?;
        let mapped =
            stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        mapped.collect::<std::result::Result<Vec<_>, _>>()?
    };
    if rows.is_empty() {
        return Ok(0);
    }
    let n = rows.len();
    for (id, enriched) in rows {
        let cleaned = strip_escaped_spans(&enriched);
        let writer = state.storage.writer();
        let conn = writer.lock().map_err(|e| anyhow::anyhow!("db lock: {e}"))?;
        conn.execute(
            "UPDATE meetings SET enriched_md = ?1 WHERE id = ?2",
            rusqlite::params![cleaned, id],
        )?;
    }
    tracing::info!(rows = n, "repaired legacy escaped-span enriched_md rows");
    Ok(n)
}

/// Remove HTML-escaped marker-span artifacts from a corrupted enriched_md
/// body. Escaped transcript-link spans go tag+content (their `↳ mm:ss`
/// text duplicates the real link span that follows); other escaped span
/// tags are stripped tag-only, keeping the sentence text between them.
fn strip_escaped_spans(s: &str) -> String {
    let mut out = s.to_string();
    // 1) `&lt;span data-transcript-link …&gt; … &lt;/span&gt;` — drop whole.
    while let Some(start) = out.find("&lt;span data-transcript-link") {
        let Some(rel_end) = out[start..].find("&lt;/span&gt;") else {
            break;
        };
        out.replace_range(start..start + rel_end + "&lt;/span&gt;".len(), "");
    }
    // 2) Any remaining escaped open tag `&lt;span …&gt;` — drop tag only.
    while let Some(start) = out.find("&lt;span") {
        let Some(rel_end) = out[start..].find("&gt;") else {
            break;
        };
        out.replace_range(start..start + rel_end + "&gt;".len(), "");
    }
    // 3) Stray escaped close tags.
    out.replace("&lt;/span&gt;", "")
}

/// Seed providers from `YOGURT_*_API_KEY` env vars.
///
/// For each preset in [`ENV_PRESETS`]:
/// 1. Skip if env var is absent or trims to empty.
/// 2. Skip (and record in `skipped`) if a provider with the same name
///    already exists (case-insensitive).
/// 3. Otherwise: insert the provider row, write the key to the key file
///    (non-fatal: a key-file write failure is logged but doesn't abort the
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

        // If a row with this name already exists, BACKFILL the stored
        // key when missing instead of skipping silently. The previous
        // behavior skipped the entire iteration — so a user who:
        //   (1) booted with an empty .env.local stub (row never inserted)
        //   (2) clicked the Minimax preset chip in Settings to scaffold
        //       a row (row inserted, no key stored)
        //   (3) added the key to .env.local and re-ran `just dev`
        // would see the provider row but `No key stored yet.` in the UI,
        // with no log line explaining why. Backfill restores the intent.
        if let Some(existing_row) = existing.iter().find(|p| p.name.eq_ignore_ascii_case(name)) {
            let existing_key = state.keys.get(&existing_row.id).ok().flatten();
            if existing_key.is_none() {
                if let Err(e) = state.keys.set(&existing_row.id, &key) {
                    tracing::warn!(provider = name, error = %e, "failed to backfill key in key store");
                    report.skipped.push(name.to_string());
                } else {
                    tracing::info!(provider = name, "backfilled missing key from env");
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

        // Store the key in the configured `ApiKeyStore` (file store in
        // production, MemoryKeyStore in tests). Non-fatal on failure —
        // the user can re-enter the key via the Settings UI.
        if let Err(e) = state.keys.set(&id, &key) {
            tracing::warn!(provider = name, error = %e, "failed to store key in key store");
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session;
    use crate::storage::Storage;
    use crate::Mode;
    use std::sync::Arc;

    /// Real corrupted sample from the pre-260813 escaped-span enhance bug:
    /// the model's `<span data-ai-grey>` / `<span data-transcript-link>`
    /// markers were HTML-escaped and then wrapped in a real span again.
    #[test]
    fn strip_escaped_spans_repairs_the_pre_260813_corruption() {
        let input = "<span data-ai-grey=\"\">&lt;span data-ai-grey data-ts=\"1\"&gt;It was decided.\
                      &lt;span data-transcript-link data-ts=\"1\"&gt;↳ 00:01&lt;/span&gt;&lt;/span&gt; \
                      <span data-transcript-link=\"\">↳ 00:01</span></span>";
        let expected = "<span data-ai-grey=\"\">It was decided. \
                         <span data-transcript-link=\"\">↳ 00:01</span></span>";
        assert_eq!(strip_escaped_spans(input), expected);
    }

    /// A clean row (no escaped span markers) must pass through unchanged —
    /// `repair_escaped_span_rows` relies on this being a true no-op so it
    /// can run unconditionally on every boot.
    #[test]
    fn strip_escaped_spans_leaves_clean_input_unchanged() {
        let clean = "<span data-ai-grey=\"\" data-ts=\"1\">It was decided.</span> \
                      <span data-transcript-link=\"\">↳ 00:01</span>";
        assert_eq!(strip_escaped_spans(clean), clean);
    }

    fn test_state() -> (AppState, tempfile::TempDir) {
        let tmp = tempfile::tempdir().unwrap();
        let storage = Arc::new(Storage::init_at(&tmp.path().join("db.sqlite")).unwrap());
        let session = Arc::new(session::load_or_create(&tmp.path().join("session-token")).unwrap());
        let notes_dir = tmp.path().join("notes");
        let state = AppState::in_memory(Mode::Release, storage, session, 7878, notes_dir)
            .expect("in_memory");
        (state, tmp)
    }

    #[tokio::test]
    async fn seed_stt_key_from_env_seeds_and_never_overwrites() {
        // SAFETY: no other unit test in this binary reads/writes
        // YOGURT_DEEPGRAM_API_KEY.
        unsafe {
            std::env::set_var("YOGURT_DEEPGRAM_API_KEY", "dg-from-env");
        }
        let (state, _tmp) = test_state();

        seed_stt_key_from_env(&state);
        let stored = state
            .keys
            .get(crate::meetings::DEEPGRAM_KEY_ID)
            .unwrap()
            .expect("key seeded from env");
        assert_eq!(stored, "dg-from-env");

        // Simulate the user having since entered their own key via
        // Settings, then re-running seeding (e.g. next boot) with a
        // DIFFERENT env value — the existing key must survive untouched.
        state
            .keys
            .set(crate::meetings::DEEPGRAM_KEY_ID, "user-entered-key")
            .unwrap();
        unsafe {
            std::env::set_var("YOGURT_DEEPGRAM_API_KEY", "dg-from-env-2");
        }
        seed_stt_key_from_env(&state);
        let stored2 = state
            .keys
            .get(crate::meetings::DEEPGRAM_KEY_ID)
            .unwrap()
            .expect("key still present");
        assert_eq!(
            stored2, "user-entered-key",
            "seed_stt_key_from_env must never overwrite an existing key"
        );

        unsafe {
            std::env::remove_var("YOGURT_DEEPGRAM_API_KEY");
        }
    }
}
