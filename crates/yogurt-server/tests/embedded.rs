//! Dual-mode asset serving: release uses the embedded `web/dist`, dev proxies to Vite at `:5173`.

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
    // Point the proxy at a dead port (bind, note, drop) so a running dev server on :5173 cannot fail the test.
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
    // The dev proxy must reject a WS upgrade with 426 rather than silently strip it.
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
    // A body over MAX_PROXY_BODY (16 MiB) must be rejected before the upstream call.
    // Not an `/api/*` path: the `api_not_found` catch-all would 404 before the body cap is reached.
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
