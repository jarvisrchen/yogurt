//! Regression: unknown `/api/*` paths must return an honest JSON 404 —
//! never a dev-mode 502 (Vite proxy miss) or a release-mode `index.html`
//! SPA fallback. `routes::router` mounts `/api/{*rest}` -> `api_not_found`
//! as the catch-all after every real `/api/*` route; this test exercises
//! it end-to-end against a real server.

use std::time::Duration;

use yogurt_server::{run_with_config, Mode, RunConfig};

async fn spawn_server() -> (
    std::net::SocketAddr,
    String,
    tokio::task::JoinHandle<()>,
    tempfile::TempDir,
) {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("yogurt-test.db");
    let token_path = tmp.path().join("session-token");

    let token = yogurt_server::session::load_or_create(&token_path)
        .expect("seed session token")
        .as_str()
        .to_string();

    let probe = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral");
    let addr = probe.local_addr().unwrap();
    drop(probe);

    std::env::set_var("YOGURT_MEMORY_KEYSTORE", "1");
    let cfg = RunConfig {
        addr,
        mode: Mode::Release,
        db_path: Some(db_path),
        session_token_path: Some(token_path),
        notes_dir: Some(tmp.path().join("notes")),
        app_db_path: Some(tmp.path().join("yogurt-app.sqlite")),
    };
    let handle = tokio::spawn(async move {
        let _ = run_with_config(cfg).await;
    });
    for _ in 0..50 {
        if reqwest::get(format!("http://{addr}/api/health"))
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
        {
            return (addr, token, handle, tmp);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("server did not become reachable within 1 second");
}

#[tokio::test(flavor = "multi_thread")]
async fn unknown_api_path_returns_json_404() {
    let (addr, token, handle, _tmp) = spawn_server().await;

    let resp = reqwest::Client::new()
        .get(format!("http://{addr}/api/definitely-not-a-route"))
        .bearer_auth(&token)
        .send()
        .await
        .expect("server reachable");

    assert_eq!(
        resp.status(),
        404,
        "unknown /api/* path must be an honest 404, not a 502 or SPA fallback"
    );
    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    assert!(
        content_type.starts_with("application/json"),
        "expected a JSON error body, got content-type: {content_type}"
    );
    let body: serde_json::Value = resp.json().await.expect("valid JSON error body");
    assert!(
        body["error"]
            .as_str()
            .map(|s| s.contains("definitely-not-a-route"))
            .unwrap_or(false),
        "error message should name the unmatched path. got: {body}"
    );

    handle.abort();
}
