//! Integration tests for the `/api/settings*` REST surface — Phase 5 Plan 05-03.
//!
//! The third test (`api_responses_never_include_the_raw_api_key`) is the
//! load-bearing security regression for the entire phase. **Never weaken
//! it.** Any change that lets a raw key escape the server in an API
//! response will trip the substring assertion below.

use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::TcpListener;

use yogurt_server::state::AppState;
use yogurt_server::{session, storage, Mode};

/// Spin up a fresh in-memory `AppState` and start an axum server on an
/// ephemeral kernel-assigned port. Returns the state handle (mostly
/// dropped), the session token, and the `http://127.0.0.1:{port}` base
/// URL to hit.
///
/// The listener is bound *before* the serve task is spawned, so incoming
/// connections queue in the accept backlog — no readiness sleep needed.
async fn boot() -> (AppState, String, String) {
    let tmp = tempfile::tempdir().expect("tempdir");
    // Keep the tempdir alive for the duration of the spawned server by
    // leaking it intentionally — this is a short-lived integration test
    // and the OS will reclaim on process exit.
    let path = tmp.keep();
    let storage = Arc::new(storage::Storage::init_at(&path.join("db.sqlite")).expect("storage"));
    let session = Arc::new(session::load_or_create(&path.join("session-token")).expect("session"));
    // The `/api/settings*` routes require the session token (hardened
    // 2026-08-13). Read it out so each request below can present it.
    let token = session.as_str().to_string();
    let notes_dir: PathBuf = path.join("notes");

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local_addr");
    let state = AppState::in_memory(Mode::Release, storage, session, addr.port(), notes_dir)
        .expect("state");

    let app = yogurt_server::__test_router(state.clone());
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (state, token, format!("http://{addr}"))
}

#[tokio::test]
async fn it_lists_seeded_settings_with_no_providers() {
    let (_state, token, base) = boot().await;
    let v: Value = reqwest::Client::new()
        .get(format!("{base}/api/settings"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(v["general"]["port"], 7878);
    assert_eq!(v["providers"].as_array().unwrap().len(), 0);
    assert!(v["presets"].as_array().unwrap().len() >= 5);
}

#[tokio::test]
async fn it_creates_a_provider_and_round_trips_via_get() {
    let (_state, token, base) = boot().await;
    let client = reqwest::Client::new();
    let created: Value = client
        .post(format!("{base}/api/settings/providers"))
        .bearer_auth(&token)
        .json(&json!({ "name": "Minimax", "base_url": "https://x/v1", "model": "M" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = created["id"].as_str().unwrap().to_string();

    let listed: Value = client
        .get(format!("{base}/api/settings/providers"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let arr = listed.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["id"], id);
    // No key has been set yet — api_key_masked must be JSON null, NEVER
    // an empty string (an empty string would imply "key set but empty",
    // which would round-trip a key-not-present state as a different shape
    // to the frontend).
    assert_eq!(arr[0]["api_key_masked"], Value::Null);
}

#[tokio::test]
async fn api_responses_never_include_the_raw_api_key() {
    // ─── Load-bearing security regression. NEVER WEAKEN. ───────────────
    // This test is the canonical proof that the raw API key never escapes
    // the server in any API response — only the canonical mask
    // (`••••XXXX`) is exposed. Any future refactor that adds an `api_key`
    // field to `ProviderView` (or otherwise echoes the raw secret) will
    // fail the `!s.contains("sk-supersecret-XYZA")` assertion below.
    let (_state, token, base) = boot().await;
    let client = reqwest::Client::new();
    let created: Value = client
        .post(format!("{base}/api/settings/providers"))
        .bearer_auth(&token)
        .json(&json!({ "name": "P", "base_url": "https://x/v1", "model": "m" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = created["id"].as_str().unwrap();

    let resp = client
        .post(format!("{base}/api/settings/providers/{id}/key"))
        .bearer_auth(&token)
        .json(&json!({ "api_key": "sk-supersecret-XYZA" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 204);

    let listed: Value = client
        .get(format!("{base}/api/settings/providers"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let s = serde_json::to_string(&listed).unwrap();
    assert!(!s.contains("sk-supersecret-XYZA"), "raw key leaked in: {s}");
    assert!(s.contains("••••XYZA"), "masked key should be present: {s}");
}

#[tokio::test]
async fn unauthenticated_requests_are_rejected() {
    // Hardening regression (2026-08-13): `/api/settings*` writes were once
    // reachable with no token — an unauthenticated cross-origin POST could
    // repoint the active LLM provider at an attacker `base_url`. Lock it:
    // a tokenless GET and a tokenless provider-create must both 403.
    let (_state, _token, base) = boot().await;
    let client = reqwest::Client::new();

    let get_status = client
        .get(format!("{base}/api/settings"))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(get_status, 403, "tokenless GET /api/settings must 403");

    let post_status = client
        .post(format!("{base}/api/settings/providers"))
        .json(&json!({ "name": "evil", "base_url": "http://attacker/v1", "model": "x" }))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(
        post_status, 403,
        "tokenless POST /api/settings/providers must 403"
    );
}
