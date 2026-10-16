//! Priority chain of `llm_openai::resolve`: `state.llm_override`, `YOGURT_LLM_*` env vars, active provider row + stored key, then `MockLlm`.
//! Reaches the `pub(crate)` module through `yogurt_server::test_support::resolve`.
//! Tests mutate process-wide env vars, so they serialize on a global mutex.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use tokio::sync::Mutex;
use yogurt_server::state::AppState;
use yogurt_server::test_support::{self, MockChunksLlm};
use yogurt_server::{session, storage, Mode};

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// SAFETY: every caller holds `env_lock()` first, so no other test in this
/// binary is concurrently reading/writing these three vars.
fn clear_llm_env() {
    unsafe {
        std::env::remove_var("YOGURT_LLM_BASE_URL");
        std::env::remove_var("YOGURT_LLM_API_KEY");
        std::env::remove_var("YOGURT_LLM_MODEL");
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

fn insert_active_provider(state: &AppState, with_key: bool) -> String {
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
    yogurt_db::providers::set_active(&state.db, &id).unwrap();
    if with_key {
        state.keys.set(&id, "sk-test").unwrap();
    }
    id
}

/// Nothing configured: falls back to `MockLlm` (identified by `model: "mock-llm"`).
#[tokio::test]
async fn no_provider_configured_resolves_to_mock() {
    let _guard = env_lock().lock().await;
    clear_llm_env();
    let (state, _tmp) = test_state();

    let llm = test_support::resolve(&state)
        .await
        .expect("resolve must succeed with nothing configured");
    let resp = llm
        .complete(yogurt_llm::ChatRequest {
            messages: vec![yogurt_llm::ChatMessage::user("hi")],
            stream: false,
        })
        .await
        .expect("mock complete must succeed");
    assert_eq!(
        resp.model, "mock-llm",
        "no configuration must resolve to MockLlm"
    );
}

#[tokio::test]
async fn active_provider_with_key_resolves_ok() {
    let _guard = env_lock().lock().await;
    clear_llm_env();
    let (state, _tmp) = test_state();
    insert_active_provider(&state, true);

    // `Arc<dyn LlmClient>` isn't `Debug`, so match explicitly.
    if let Err(e) = test_support::resolve(&state).await {
        panic!("provider + key must resolve Ok, got error: {e}");
    }
}

/// Active provider row but no key: hard Err naming the provider, never a silent mock fallback.
#[tokio::test]
async fn active_provider_without_key_errors_naming_provider() {
    let _guard = env_lock().lock().await;
    clear_llm_env();
    let (state, _tmp) = test_state();
    insert_active_provider(&state, false);

    // `Result::expect_err` requires `T: Debug`, so match explicitly.
    let err = match test_support::resolve(&state).await {
        Ok(_) => panic!("missing key must be a hard Err, not a silent mock fallback"),
        Err(e) => e,
    };
    let msg = err.to_string();
    assert!(
        msg.contains("Minimax"),
        "error must name the provider: {msg}"
    );
}

/// `state.llm_override` wins over env vars and an active provider + key.
#[tokio::test]
async fn llm_override_wins_over_env_and_provider() {
    let _guard = env_lock().lock().await;
    unsafe {
        std::env::set_var("YOGURT_LLM_BASE_URL", "http://127.0.0.1:1");
        std::env::set_var("YOGURT_LLM_API_KEY", "sk-env");
        std::env::set_var("YOGURT_LLM_MODEL", "env-model");
    }
    let (mut state, _tmp) = test_state();
    insert_active_provider(&state, true);
    state.llm_override = Some(Arc::new(MockChunksLlm::new(&["OVERRIDE-MARKER"])));

    let llm = test_support::resolve(&state)
        .await
        .expect("resolve must succeed");
    let resp = llm
        .complete(yogurt_llm::ChatRequest {
            messages: vec![yogurt_llm::ChatMessage::user("hi")],
            stream: false,
        })
        .await
        .expect("override complete must succeed");
    assert_eq!(
        resp.content, "OVERRIDE-MARKER",
        "llm_override must win over both env vars and the active provider"
    );

    clear_llm_env();
}
