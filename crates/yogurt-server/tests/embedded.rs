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
