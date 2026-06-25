use std::time::Duration;

#[tokio::test]
async fn it_serves_embedded_index_in_release_mode() {
    let addr = "127.0.0.1:17880".parse().unwrap();
    let handle =
        tokio::spawn(async move { yogurt_server::run(addr, yogurt_server::Mode::Release).await });
    tokio::time::sleep(Duration::from_millis(200)).await;

    let body = reqwest::get("http://127.0.0.1:17880/")
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
    let addr = "127.0.0.1:17881".parse().unwrap();
    let handle =
        tokio::spawn(async move { yogurt_server::run(addr, yogurt_server::Mode::Dev).await });
    tokio::time::sleep(Duration::from_millis(200)).await;

    let resp = reqwest::get("http://127.0.0.1:17881/")
        .await
        .expect("server reachable");
    assert_eq!(
        resp.status(),
        502,
        "no vite running on :5173 should produce a 502 from the dev proxy"
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
    let addr = "127.0.0.1:17884".parse().unwrap();
    let handle =
        tokio::spawn(async move { yogurt_server::run(addr, yogurt_server::Mode::Dev).await });
    tokio::time::sleep(Duration::from_millis(200)).await;

    let client = reqwest::Client::new();
    let resp = client
        .get("http://127.0.0.1:17884/__vite_hmr")
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
    let addr = "127.0.0.1:17885".parse().unwrap();
    let handle =
        tokio::spawn(async move { yogurt_server::run(addr, yogurt_server::Mode::Dev).await });
    tokio::time::sleep(Duration::from_millis(200)).await;

    let huge = vec![b'x'; 16 * 1024 * 1024 + 1];
    let client = reqwest::Client::new();
    let resp = client
        .post("http://127.0.0.1:17885/api/upload-imaginary")
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
