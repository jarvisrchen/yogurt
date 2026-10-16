//! `seed_from_env` env-var to provider and key store mapping.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use tokio::sync::Mutex;

use yogurt_server::state::AppState;
use yogurt_server::{bootstrap, session, storage, Mode};

/// Serializes env-var mutation across parallel tests. A tokio mutex so the guard can be held across `.await` without `clippy::await_holding_lock`.
fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Clears every `YOGURT_*_API_KEY` the bootstrap reads so leftovers from sibling tests or the developer's shell cannot seed extra providers.
/// Driven off `bootstrap::env_key_vars()` so new presets are covered automatically.
fn clear_provider_env() {
    for var in bootstrap::env_key_vars() {
        std::env::remove_var(var);
    }
}

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
    clear_provider_env();

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
    clear_provider_env();

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

    let providers = yogurt_db::providers::list(&state.db).unwrap();
    let minimax_count = providers
        .iter()
        .filter(|p| p.name.eq_ignore_ascii_case("Minimax"))
        .count();
    assert_eq!(minimax_count, 1, "exactly one Minimax row");

    std::env::remove_var("YOGURT_MINIMAX_API_KEY");
}

/// Regression: a provider row that exists with no stored key (created from a Settings preset chip) must get its key backfilled, not skipped.
#[tokio::test]
async fn it_backfills_missing_key_when_row_exists() {
    let _guard = env_lock().lock().await;
    clear_provider_env();

    let (state, _tmp) = test_state();

    // A row without a key, as the UI preset-clone flow leaves it.
    let id = yogurt_db::providers::insert(
        &state.db,
        yogurt_db::providers::NewProvider {
            name: "Minimax".to_string(),
            base_url: "https://api.minimax.io/v1".to_string(),
            model: "MiniMax-Text-01".to_string(),
            adapter: yogurt_db::providers::adapter::HTTP.to_string(),
            cli_model: String::new(),
        },
    )
    .unwrap();
    assert!(
        state.keys.get(&id).unwrap().is_none(),
        "precondition: no key stored"
    );

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
    clear_provider_env();

    std::env::set_var("YOGURT_MINIMAX_API_KEY", "sk-test-minimax-12345");
    std::env::set_var("YOGURT_OPENAI_API_KEY", "sk-test-openai-67890");
    let (state, _tmp) = test_state();
    bootstrap::seed_from_env(&state).await.unwrap();

    let active = yogurt_db::providers::active(&state.db).unwrap().unwrap();
    assert_eq!(active.name, "Minimax");

    let all = yogurt_db::providers::list(&state.db).unwrap();
    let openai = all
        .iter()
        .find(|p| p.name == "OpenAI")
        .expect("OpenAI present");
    assert!(!openai.is_active, "OpenAI must not be active");

    let openai_key = state.keys.get(&openai.id).unwrap().expect("openai key");
    assert_eq!(openai_key, "sk-test-openai-67890");

    std::env::remove_var("YOGURT_MINIMAX_API_KEY");
    std::env::remove_var("YOGURT_OPENAI_API_KEY");
}

/// The `.env.local` loader in `yogurt-cli/src/main.rs` must stay behind the `--dev` arg check so release builds never read it.
#[test]
fn cli_main_only_loads_env_local_when_dev_flag_passed() {
    let main_rs =
        std::fs::read_to_string("../yogurt-cli/src/main.rs").expect("read yogurt-cli/src/main.rs");

    assert!(
        main_rs.contains("dotenvy::from_filename(\".env.local\")"),
        "main.rs must call dotenvy::from_filename(\".env.local\")"
    );

    assert!(
        !main_rs.contains("dotenvy::dotenv()"),
        "main.rs MUST NOT call dotenvy::dotenv() — that reads .env unconditionally"
    );

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
