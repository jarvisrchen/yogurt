//! Dual-mode asset serving tests: release uses the embedded `web/dist`,
//! dev proxies to Vite at `:5173`.
//!
//! Phase 5 (SET-12) refactor: tests now use `run_with_config` with tempdir
//! and ephemeral-port isolation (matching the audio_api / meeting_rest
//! pattern) so they no longer collide on hardcoded ports 17880..17885 and
//! no longer touch the real `~/.yogurt/db.sqlite`.

use std::net::SocketAddr;
use std::time::Duration;
use yogurt_server::{run_with_config, Mode, RunConfig};

async fn spawn_server(mode: Mode) -> (SocketAddr, tokio::task::JoinHandle<()>, tempfile::TempDir) {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let probe = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral");
    let addr: SocketAddr = probe.local_addr().expect("local_addr");
    drop(probe);

    std::env::set_var("YOGURT_MEMORY_KEYSTORE", "1");
    let cfg = RunConfig {
        addr,
        mode,
        db_path: Some(tmp.path().join("yogurt-test.db")),
        session_token_path: Some(tmp.path().join("session-token")),
        notes_dir: Some(tmp.path().join("notes")),
        // Phase 5 (SET-12): tempdir-isolate the new yogurt-db.
        app_db_path: Some(tmp.path().join("yogurt-app.sqlite")),
    };
    let handle = tokio::spawn(async move {
        let _ = run_with_config(cfg).await;
    });

    // Poll until the server is up — /api/health is unauthenticated.
    for _ in 0..50 {
        if reqwest::get(format!("http://{addr}/api/health"))
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
        {
            return (addr, handle, tmp);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("server did not become reachable within 1 second");
}

#[tokio::test]
async fn it_serves_embedded_index_in_release_mode() {
    let (addr, handle, _tmp) = spawn_server(Mode::Release).await;

    let body = reqwest::get(format!("http://{addr}/"))
        .await
        .expect("server reachable")
        .text()
        .await
        .expect("body readable");

    assert!(
        body.contains("yogurt"),
        "embedded index should mention yogurt; got: {body}"
    );
    handle.abort();
}

#[tokio::test]
async fn it_returns_bad_gateway_in_dev_mode_when_vite_is_down() {
    // Point the proxy at a port that is guaranteed dead (bind an ephemeral
    // listener, note its port, drop it) instead of asserting nothing runs
    // on the real :5173 - a legitimately-running dev server on this machine
    // used to fail this test.
    let probe = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let dead_port = probe.local_addr().unwrap().port();
    drop(probe);
    std::env::set_var("YOGURT_VITE_BASE", format!("http://127.0.0.1:{dead_port}"));

    let (addr, handle, _tmp) = spawn_server(Mode::Dev).await;

    let resp = reqwest::get(format!("http://{addr}/"))
        .await
        .expect("server reachable");
    assert_eq!(
        resp.status(),
        502,
        "a dead vite target must produce a 502 from the dev proxy"
    );

    let body = resp.text().await.expect("body readable");
    assert!(
        body.contains("pnpm --dir web dev"),
        "502 body should tell the user how to start vite; got: {body}"
    );

    handle.abort();
}

#[tokio::test]
async fn it_rejects_websocket_upgrade_through_dev_proxy_with_426() {
    // HI-04: dev proxy must NOT silently strip a WS upgrade. It either has
    // to forward the WS upgrade (out of Phase 0 scope) or reject with 426
    // Upgrade Required. We chose the latter.
    let (addr, handle, _tmp) = spawn_server(Mode::Dev).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("http://{addr}/__vite_hmr"))
        .header("connection", "upgrade")
        .header("upgrade", "websocket")
        .header("sec-websocket-version", "13")
        .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
        .send()
        .await
        .expect("server reachable");
    assert_eq!(
        resp.status(),
        426,
        "WS upgrade through dev proxy must return 426 Upgrade Required"
    );
    let body = resp.text().await.expect("body");
    assert!(
        body.contains("localhost:5173"),
        "426 body should point user at Vite; got: {body}"
    );

    handle.abort();
}

#[tokio::test]
async fn it_rejects_oversized_request_body_with_413() {
    // HI-03: a body larger than MAX_PROXY_BODY (16 MiB) must be rejected.
    // We post just over the limit with a body the dev proxy will try to
    // buffer; Vite doesn't need to be running because the body cap fires
    // before the upstream call.
    //
    // Regression note: this must NOT be an `/api/*` path. `routes::router`
    // now mounts `/api/{*rest}` -> `api_not_found` as a catch-all ahead of
    // this SPA/dev-proxy fallback, so an `/api/*` path would 404 instantly
    // without ever reaching `proxy_to_vite`'s body-size cap — this test
    // used to post to `/api/upload-imaginary` and silently stopped
    // exercising HI-03 the moment that catch-all landed.
    let (addr, handle, _tmp) = spawn_server(Mode::Dev).await;

    let huge = vec![b'x'; 16 * 1024 * 1024 + 1];
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://{addr}/upload-imaginary"))
        .body(huge)
        .send()
        .await
        .expect("server reachable");
    assert_eq!(
        resp.status(),
        413,
        "oversized body must produce 413 Payload Too Large"
    );

    handle.abort();
}
