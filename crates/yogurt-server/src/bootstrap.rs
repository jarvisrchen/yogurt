//! Seed providers from `YOGURT_*_API_KEY` env vars on first run.
//!
//! Idempotent: existing rows (matched case-insensitively by name) are never overwritten.
//! The first LLM provider seeded becomes active iff none was already active.
//!
//! `seed_from_env` only reads `std::env`. The `.env.local` loader lives in
//! `yogurt-cli/src/main.rs` behind `--dev`, so a release build without those vars is a no-op
//! and seeding can run on every boot.

use crate::state::AppState;
use anyhow::Result;
use yogurt_db::providers::{self, NewProvider};

/// `(env_var, provider_name, base_url, default_model)`. LLM providers only.
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

/// Exposed so `tests/bootstrap.rs` can clear every preset var rather than hand-list them.
pub fn env_key_vars() -> impl Iterator<Item = &'static str> {
    ENV_PRESETS
        .iter()
        .map(|&(var, ..)| var)
        .chain([GRANOLA_ENV])
}

const GRANOLA_ENV: &str = "YOGURT_GRANOLA_API_KEY";
/// The `keys.json` account for the Granola import key.
pub(crate) const GRANOLA_ACCOUNT: &str = "import-granola";

/// Seeds the Granola import key from `YOGURT_GRANOLA_API_KEY`. Like the Deepgram seed, it never overwrites a stored key.
pub fn seed_granola_key_from_env(state: &AppState) {
    let Ok(key) = std::env::var(GRANOLA_ENV) else {
        return;
    };
    if key.trim().is_empty() || state.keys.get(GRANOLA_ACCOUNT).ok().flatten().is_some() {
        return;
    }
    match state.keys.set(GRANOLA_ACCOUNT, key.trim()) {
        Ok(()) => tracing::info!("seeded Granola import key from env"),
        Err(e) => tracing::warn!(error = %e, "failed to seed Granola import key"),
    }
}

#[derive(Debug, Default)]
pub struct SeedReport {
    pub seeded: Vec<String>,
    pub skipped: Vec<String>,
}

/// Seeds the Deepgram key from `YOGURT_DEEPGRAM_API_KEY`. An existing stored key is never
/// overwritten because Settings owns it once set. STT keys have no `providers` row.
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

/// Repairs `enriched_md` rows where marker spans were HTML-escaped and re-wrapped, leaving
/// literal `&lt;span data-ai-grey ...&gt;` text. Idempotent and cheap, so it runs on every boot.
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

/// Escaped transcript-link spans are dropped whole because their `↳ mm:ss` text duplicates the
/// real link that follows. Other escaped span tags are stripped, keeping the text between them.
fn strip_escaped_spans(s: &str) -> String {
    let mut out = s.to_string();
    while let Some(start) = out.find("&lt;span data-transcript-link") {
        let Some(rel_end) = out[start..].find("&lt;/span&gt;") else {
            break;
        };
        out.replace_range(start..start + rel_end + "&lt;/span&gt;".len(), "");
    }
    while let Some(start) = out.find("&lt;span") {
        let Some(rel_end) = out[start..].find("&gt;") else {
            break;
        };
        out.replace_range(start..start + rel_end + "&gt;".len(), "");
    }
    out.replace("&lt;/span&gt;", "")
}

/// A key-store write failure is logged but does not abort, since the user can re-enter the key
/// in Settings.
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

        // Backfill a missing key on an existing row. Otherwise a row scaffolded from a Settings
        // preset chip before the key reached .env.local shows "No key stored yet." with no log.
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
                adapter: providers::adapter::HTTP.to_string(),
                cli_model: String::new(),
            },
        )?;

        if let Err(e) = state.keys.set(&id, &key) {
            tracing::warn!(provider = name, error = %e, "failed to store key in key store");
        }

        // Checked after each insert so the first preset in ENV_PRESETS wins.
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

    /// Real corrupted sample: marker spans HTML-escaped, then wrapped in a real span again.
    #[test]
    fn strip_escaped_spans_repairs_the_pre_260813_corruption() {
        let input = "<span data-ai-grey=\"\">&lt;span data-ai-grey data-ts=\"1\"&gt;It was decided.\
                      &lt;span data-transcript-link data-ts=\"1\"&gt;↳ 00:01&lt;/span&gt;&lt;/span&gt; \
                      <span data-transcript-link=\"\">↳ 00:01</span></span>";
        let expected = "<span data-ai-grey=\"\">It was decided. \
                         <span data-transcript-link=\"\">↳ 00:01</span></span>";
        assert_eq!(strip_escaped_spans(input), expected);
    }

    /// `repair_escaped_span_rows` runs on every boot, so a clean row must pass through unchanged.
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
    async fn seed_granola_key_from_env_seeds_and_never_overwrites() {
        // SAFETY: no other unit test in this binary touches YOGURT_GRANOLA_API_KEY.
        unsafe {
            std::env::set_var(GRANOLA_ENV, " grn_from_env ");
        }
        let (state, _tmp) = test_state();
        seed_granola_key_from_env(&state);
        assert_eq!(
            state.keys.get(GRANOLA_ACCOUNT).unwrap().as_deref(),
            Some("grn_from_env")
        );
        state.keys.set(GRANOLA_ACCOUNT, "user_entered").unwrap();
        seed_granola_key_from_env(&state);
        assert_eq!(
            state.keys.get(GRANOLA_ACCOUNT).unwrap().as_deref(),
            Some("user_entered")
        );
        unsafe {
            std::env::remove_var(GRANOLA_ENV);
        }
    }

    #[tokio::test]
    async fn seed_stt_key_from_env_seeds_and_never_overwrites() {
        // SAFETY: no other unit test in this binary touches YOGURT_DEEPGRAM_API_KEY.
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

        // A user-entered key must survive re-seeding with a different env value.
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
