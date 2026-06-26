//! REST lifecycle tests for `/api/meetings*` (Phase 3 / Plan 03-02 Task 2).
//!
//! Two tests on ports 17890 / 17891 (reserved across the Phase 3 plan to
//! avoid collisions with `meeting_ws.rs` (17892) and `e2e_synthetic_audio.rs`
//! (17893)):
//!
//! - `it_creates_a_meeting_and_returns_an_id` — happy path; asserts UUID
//!   parses and `created_at_ms > 0`.
//! - `it_rejects_start_without_api_key` — D-07 contract; with the env var
//!   unset, `POST /start` returns 400 and the body contains the env var name
//!   so the UI / CLI can render a helpful "set this and retry" prompt.

use std::time::Duration;

use yogurt_server::{run_with_config, Mode, RunConfig};

/// Spawn a fresh server bound to `addr` with temp DB + temp session token
/// (so the test never touches the developer's real `~/.yogurt/`).
async fn spawn_server(addr: std::net::SocketAddr) -> (tokio::task::JoinHandle<()>, tempfile::TempDir)
{
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("yogurt-test.db");
    let token_path = tmp.path().join("session-token");
    let cfg = RunConfig {
        addr,
        mode: Mode::Release,
        db_path: Some(db_path),
        session_token_path: Some(token_path),
    };
    let handle = tokio::spawn(async move {
        let _ = run_with_config(cfg).await;
    });
    // Give the server a moment to bind.
    tokio::time::sleep(Duration::from_millis(200)).await;
    (handle, tmp)
}

#[tokio::test(flavor = "multi_thread")]
async fn it_creates_a_meeting_and_returns_an_id() {
    let addr: std::net::SocketAddr = "127.0.0.1:17890".parse().unwrap();
    let (handle, _tmp) = spawn_server(addr).await;

    let body = reqwest::Client::new()
        .post("http://127.0.0.1:17890/api/meetings")
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();

    let id = body["id"].as_str().expect("id is a string");
    assert!(uuid::Uuid::parse_str(id).is_ok(), "id parses as uuid");
    assert!(body["created_at_ms"].as_u64().unwrap() > 0);

    handle.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn it_rejects_start_without_api_key() {
    // Ensure the env var is NOT set for this test. SAFETY: tests in the
    // same binary share the process env; this test pair is sequential.
    // SAFETY: removing a possibly-unset env var is sound; we accept the
    // cross-test contamination risk because the planner reserved these
    // two tests for this binary specifically.
    unsafe {
        std::env::remove_var("YOGURT_DEEPGRAM_API_KEY");
    }

    let addr: std::net::SocketAddr = "127.0.0.1:17891".parse().unwrap();
    let (handle, _tmp) = spawn_server(addr).await;

    let client = reqwest::Client::new();
    let created = client
        .post("http://127.0.0.1:17891/api/meetings")
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    let id = created["id"].as_str().unwrap();

    let resp = client
        .post(format!("http://127.0.0.1:17891/api/meetings/{id}/start"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let body = resp.json::<serde_json::Value>().await.unwrap();
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("YOGURT_DEEPGRAM_API_KEY"));

    handle.abort();
}
