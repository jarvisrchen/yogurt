use std::time::Duration;

#[tokio::test]
async fn it_responds_to_health() {
    let addr = "127.0.0.1:17878".parse().unwrap();
    let mode = yogurt_server::Mode::Release;

    // Spawn the server.
    let handle = tokio::spawn(async move { yogurt_server::run(addr, mode).await });

    // Give it a moment to bind.
    tokio::time::sleep(Duration::from_millis(200)).await;

    let body = reqwest::get("http://127.0.0.1:17878/api/health")
        .await
        .expect("server reachable")
        .json::<serde_json::Value>()
        .await
        .expect("valid JSON");

    assert_eq!(body["status"], "ok");
    assert_eq!(body["service"], "yogurt-server");
    handle.abort();
}
