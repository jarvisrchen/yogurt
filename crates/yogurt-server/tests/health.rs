//! Smoke test for the unauthenticated `/api/health` endpoint.
//!
//! Phase 5 (SET-12) refactor: use `run_with_config` with tempdir +
//! ephemeral-port isolation so the test no longer touches the real
//! `~/.yogurt/db.sqlite` and no longer collides on a hardcoded port.

use std::net::SocketAddr;
use std::time::Duration;
use yogurt_server::{run_with_config, Mode, RunConfig};

#[tokio::test]
async fn it_responds_to_health() {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let probe = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral");
    let addr: SocketAddr = probe.local_addr().expect("local_addr");
    drop(probe);

    std::env::set_var("YOGURT_MEMORY_KEYSTORE", "1");
    let cfg = RunConfig {
        addr,
        mode: Mode::Release,
        db_path: Some(tmp.path().join("yogurt-test.db")),
        session_token_path: Some(tmp.path().join("session-token")),
        notes_dir: Some(tmp.path().join("notes")),
        // Phase 5 (SET-12): tempdir-isolate the new yogurt-db.
        app_db_path: Some(tmp.path().join("yogurt-app.sqlite")),
    };
    let handle = tokio::spawn(async move {
        let _ = run_with_config(cfg).await;
    });

    // Poll until the server is up.
    for _ in 0..50 {
        if reqwest::get(format!("http://{addr}/api/health"))
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let body = reqwest::get(format!("http://{addr}/api/health"))
        .await
        .expect("server reachable")
        .json::<serde_json::Value>()
        .await
        .expect("valid JSON");

    assert_eq!(body["status"], "ok");
    assert_eq!(body["service"], "yogurt-server");
    handle.abort();
}
