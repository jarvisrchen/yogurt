//! Bootstrap tests — `seed_from_env` env-var → providers + Keychain mapping.
//!
//! Phase 5 (Plan 05-02) Task 3. Asserts:
//! 1. `YOGURT_MINIMAX_API_KEY` env var seeds a Minimax provider, sets the
//!    key in the (memory) keystore, and marks the row active.
//! 2. `seed_from_env` is idempotent — running twice never duplicates rows
//!    and the second run reports the existing names as skipped.
//! 3. The first-seeded LLM provider wins the active slot — subsequent
//!    seeds in the same run do NOT override an already-active provider.
//!
//! These tests serialize on `std::env::set_var` / `remove_var` so they MUST
//! run sequentially (cargo's default for integration tests within a single
//! file). The mutex below enforces serialization explicitly so flaky
//! parallel-runner setups don't interleave the env vars.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use tokio::sync::Mutex;

use yogurt_server::state::AppState;
use yogurt_server::{bootstrap, session, storage, Mode};

/// Process-global mutex to prevent env-var races even when the test runner
/// hands tests to parallel threads. Without this, two `set_var` calls from
/// concurrent tests can leak the wrong key into the wrong assertion.
///
/// `tokio::sync::Mutex` (not `std::sync::Mutex`) so the guard can be held
/// across `seed_from_env(...).await` without tripping
/// `clippy::await_holding_lock`.
fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Create an isolated AppState rooted in a tempdir. Returns the state + the
/// tempdir guard so the caller can drop it at end-of-test to clean up.
fn test_state() -> (AppState, tempfile::TempDir) {
    let tmp = tempfile::tempdir().unwrap();
    let storage = Arc::new(storage::Storage::init_at(&tmp.path().join("db.sqlite")).unwrap());
    let session = Arc::new(session::load_or_create(&tmp.path().join("session-token")).unwrap());
    let notes_dir: PathBuf = tmp.path().join("notes");
    let state =
        AppState::in_memory(Mode::Release, storage, session, 7878, notes_dir).expect("in_memory");
    (state, tmp)
}

#[tokio::test]
async fn it_seeds_minimax_from_env() {
    let _guard = env_lock().lock().await;
    // Belt-and-braces: clear other YOGURT_*_API_KEY env vars so a leftover
    // from a sibling test (or the user's shell) doesn't add unintended
    // providers and break the "single Minimax seeded" assertion.
    std::env::remove_var("YOGURT_OPENAI_API_KEY");
    std::env::remove_var("YOGURT_OPENROUTER_API_KEY");

    std::env::set_var("YOGURT_MINIMAX_API_KEY", "sk-test-minimax-12345");
    let (state, _tmp) = test_state();
    let report = bootstrap::seed_from_env(&state).await.expect("ok");
    assert_eq!(report.seeded, vec!["Minimax".to_string()]);

    let active = yogurt_db::providers::active(&state.db)
        .unwrap()
        .expect("active provider");
    assert_eq!(active.name, "Minimax");
    assert_eq!(active.base_url, "https://api.minimax.io/v1");

    let stored_key = state.keys.get(&active.id).unwrap().expect("key in store");
    assert_eq!(stored_key, "sk-test-minimax-12345");

    std::env::remove_var("YOGURT_MINIMAX_API_KEY");
}

#[tokio::test]
async fn it_is_idempotent() {
    let _guard = env_lock().lock().await;
    std::env::remove_var("YOGURT_OPENAI_API_KEY");
    std::env::remove_var("YOGURT_OPENROUTER_API_KEY");

    std::env::set_var("YOGURT_MINIMAX_API_KEY", "sk-test-minimax-12345");
    let (state, _tmp) = test_state();
    let first = bootstrap::seed_from_env(&state).await.unwrap();
    assert_eq!(first.seeded, vec!["Minimax".to_string()]);
    assert!(first.skipped.is_empty());

    let second = bootstrap::seed_from_env(&state).await.unwrap();
    assert!(
        second.seeded.is_empty(),
        "second run should not seed Minimax again, got {:?}",
        second.seeded
    );
    assert_eq!(second.skipped, vec!["Minimax".to_string()]);

    // Row count must remain at exactly one Minimax row.
    let providers = yogurt_db::providers::list(&state.db).unwrap();
    let minimax_count = providers
        .iter()
        .filter(|p| p.name.eq_ignore_ascii_case("Minimax"))
        .count();
    assert_eq!(minimax_count, 1, "exactly one Minimax row");

    std::env::remove_var("YOGURT_MINIMAX_API_KEY");
}

/// REGRESSION: if a provider row already exists with NO key in the
/// Keychain (e.g. the user clicked a preset chip in Settings to scaffold
/// the row before adding the env var), the seed MUST backfill the key
/// instead of silently leaving the row keyless.
///
/// The original behavior skipped the entire iteration when a same-named
/// row existed, which left users staring at "No key stored yet." in
/// Settings after a `just dev` boot that should have fixed it.
#[tokio::test]
async fn it_backfills_missing_keychain_key_when_row_exists() {
    let _guard = env_lock().lock().await;
    std::env::remove_var("YOGURT_OPENAI_API_KEY");
    std::env::remove_var("YOGURT_OPENROUTER_API_KEY");
    std::env::remove_var("YOGURT_MINIMAX_API_KEY");

    let (state, _tmp) = test_state();

    // Simulate "user clicked the Minimax preset chip" — insert the row
    // without ever writing a key. This is the exact state the UI leaves
    // the DB in after the preset-clone flow.
    let id = yogurt_db::providers::insert(
        &state.db,
        yogurt_db::providers::NewProvider {
            name: "Minimax".to_string(),
            base_url: "https://api.minimax.io/v1".to_string(),
            model: "MiniMax-Text-01".to_string(),
        },
    )
    .unwrap();
    assert!(
        state.keys.get(&id).unwrap().is_none(),
        "precondition: no key in Keychain"
    );

    // User then adds the env var and re-runs `just dev`.
    std::env::set_var("YOGURT_MINIMAX_API_KEY", "sk-backfilled-12345");
    let report = bootstrap::seed_from_env(&state).await.unwrap();

    assert_eq!(
        report.seeded,
        vec!["Minimax".to_string()],
        "backfill should count as a seed action, not a skip"
    );
    assert!(report.skipped.is_empty());

    let stored = state.keys.get(&id).unwrap().expect("key backfilled");
    assert_eq!(stored, "sk-backfilled-12345");

    // Second seed with the key already in place must NOT touch the
    // Keychain (no spurious overwrite, no duplicate row).
    let second = bootstrap::seed_from_env(&state).await.unwrap();
    assert!(second.seeded.is_empty(), "second seed should not re-write");
    assert_eq!(second.skipped, vec!["Minimax".to_string()]);

    let providers = yogurt_db::providers::list(&state.db).unwrap();
    let minimax_count = providers
        .iter()
        .filter(|p| p.name.eq_ignore_ascii_case("Minimax"))
        .count();
    assert_eq!(minimax_count, 1, "exactly one Minimax row");

    std::env::remove_var("YOGURT_MINIMAX_API_KEY");
}

#[tokio::test]
async fn it_does_not_override_existing_active() {
    let _guard = env_lock().lock().await;
    std::env::remove_var("YOGURT_OPENROUTER_API_KEY");

    std::env::set_var("YOGURT_MINIMAX_API_KEY", "sk-test-minimax-12345");
    std::env::set_var("YOGURT_OPENAI_API_KEY", "sk-test-openai-67890");
    let (state, _tmp) = test_state();
    bootstrap::seed_from_env(&state).await.unwrap();

    // Minimax is first in ENV_PRESETS → wins the active slot.
    // OpenAI is configured but inactive.
    let active = yogurt_db::providers::active(&state.db).unwrap().unwrap();
    assert_eq!(active.name, "Minimax");

    let all = yogurt_db::providers::list(&state.db).unwrap();
    let openai = all
        .iter()
        .find(|p| p.name == "OpenAI")
        .expect("OpenAI present");
    assert!(!openai.is_active, "OpenAI must not be active");

    // OpenAI's key still got stored.
    let openai_key = state.keys.get(&openai.id).unwrap().expect("openai key");
    assert_eq!(openai_key, "sk-test-openai-67890");

    std::env::remove_var("YOGURT_MINIMAX_API_KEY");
    std::env::remove_var("YOGURT_OPENAI_API_KEY");
}

/// Release-safety verification (SET-11): the `.env.local` loader in
/// `yogurt-cli/src/main.rs` must be guarded by an `--dev` arg check, NOT
/// called unconditionally. A drift here would leak `.env.local` reads into
/// brew-install release builds. Grep the file contents and assert the
/// guard is present.
#[test]
fn cli_main_only_loads_env_local_when_dev_flag_passed() {
    let main_rs =
        std::fs::read_to_string("../yogurt-cli/src/main.rs").expect("read yogurt-cli/src/main.rs");

    // Must contain the load call with the exact filename.
    assert!(
        main_rs.contains("dotenvy::from_filename(\".env.local\")"),
        "main.rs must call dotenvy::from_filename(\".env.local\")"
    );

    // Must NOT call the unconditional dotenv()/dotenv_iter() variants.
    assert!(
        !main_rs.contains("dotenvy::dotenv()"),
        "main.rs MUST NOT call dotenvy::dotenv() — that reads .env unconditionally"
    );

    // The from_filename call must appear AFTER a --dev arg check. We check
    // this conservatively: find the position of the --dev check and ensure
    // it comes before the loader call.
    let dev_check_pos = main_rs
        .find("std::env::args")
        .or_else(|| main_rs.find(r#""--dev""#))
        .expect("main.rs must contain a --dev arg check guarding the loader");
    let loader_pos = main_rs
        .find("dotenvy::from_filename(\".env.local\")")
        .unwrap();
    assert!(
        dev_check_pos < loader_pos,
        "--dev arg check must appear before dotenvy::from_filename in main.rs"
    );
}
