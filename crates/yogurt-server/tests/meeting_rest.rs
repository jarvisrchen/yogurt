//! REST lifecycle tests for `/api/meetings*`.

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
        // Parallel tests deadlock on the real user DB's WAL lock.
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
async fn it_creates_a_meeting_and_returns_an_id() {
    let (addr, token, handle, _tmp) = spawn_server().await;

    let body = reqwest::Client::new()
        .post(format!("http://{addr}/api/meetings"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();

    let id = body["id"].as_str().expect("id is a string");
    assert!(uuid::Uuid::parse_str(id).is_ok(), "id parses as uuid");
    assert!(
        body["created_at"]
            .as_str()
            .map(|s| !s.is_empty())
            .unwrap_or(false),
        "created_at is a non-empty ISO 8601 string"
    );
    let started_at = body["started_at"].as_i64().unwrap();
    assert_eq!(
        body["title"].as_str().unwrap(),
        format!("Meeting {}", yogurt_db::meetings::short_date(started_at))
    );

    handle.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn it_rejects_start_without_api_key() {
    // SAFETY: tests in the same binary share the process env, and this test pair is sequential.
    unsafe {
        std::env::remove_var("YOGURT_DEEPGRAM_API_KEY");
    }

    let (addr, token, handle, _tmp) = spawn_server().await;

    let client = reqwest::Client::new();
    let created = client
        .post(format!("http://{addr}/api/meetings"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    let id = created["id"].as_str().unwrap();

    let resp = client
        .post(format!("http://{addr}/api/meetings/{id}/start"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let body = resp.json::<serde_json::Value>().await.unwrap();
    let msg = body["error"].as_str().unwrap().to_lowercase();
    assert!(
        msg.contains("api key"),
        "error should mention the missing API key: {msg}"
    );
    assert!(
        msg.contains("settings"),
        "error should point the user at Settings: {msg}"
    );

    handle.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn it_rejects_meeting_endpoints_without_token() {
    let (addr, _token, handle, _tmp) = spawn_server().await;

    let resp = reqwest::Client::new()
        .post(format!("http://{addr}/api/meetings"))
        .send()
        .await
        .expect("server reachable");
    assert_eq!(resp.status(), 403);

    handle.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn it_serves_session_token_to_allowed_origin() {
    let (addr, token, handle, _tmp) = spawn_server().await;

    let resp = reqwest::Client::new()
        .get(format!("http://{addr}/api/session-token"))
        .header("origin", format!("http://127.0.0.1:{}", addr.port()))
        .send()
        .await
        .expect("server reachable");
    assert_eq!(resp.status(), 200);
    let body = resp.json::<serde_json::Value>().await.unwrap();
    assert_eq!(body["token"], token);

    handle.abort();
}

/// Regression: browsers omit `Origin` on same-origin GET, so the bootstrap must accept an empty Origin with `Sec-Fetch-Site: same-origin`, or the SPA cannot fetch its token.
#[tokio::test(flavor = "multi_thread")]
async fn it_accepts_same_origin_bootstrap_with_empty_origin() {
    let (addr, token, handle, _tmp) = spawn_server().await;

    let resp = reqwest::Client::new()
        .get(format!("http://{addr}/api/session-token"))
        .header("Sec-Fetch-Site", "same-origin")
        .send()
        .await
        .expect("server reachable");
    assert_eq!(
        resp.status(),
        200,
        "browser same-origin GET (no Origin, Sec-Fetch-Site: same-origin) must be accepted"
    );
    let body = resp.json::<serde_json::Value>().await.unwrap();
    assert_eq!(body["token"], token);

    handle.abort();
}

/// Regression: cross-site `Sec-Fetch-Site` (unforgeable) with no Origin must still 403, so an attacker iframe cannot read the token.
#[tokio::test(flavor = "multi_thread")]
async fn it_rejects_cross_site_bootstrap_with_empty_origin() {
    let (addr, _token, handle, _tmp) = spawn_server().await;

    let resp = reqwest::Client::new()
        .get(format!("http://{addr}/api/session-token"))
        .header("Sec-Fetch-Site", "cross-site")
        .send()
        .await
        .expect("server reachable");
    assert_eq!(resp.status(), 403);

    handle.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn it_rejects_session_token_for_bad_origin() {
    let (addr, _token, handle, _tmp) = spawn_server().await;

    let resp = reqwest::Client::new()
        .get(format!("http://{addr}/api/session-token"))
        .header("origin", "http://evil.example.com")
        .send()
        .await
        .expect("server reachable");
    assert_eq!(resp.status(), 403);

    handle.abort();
}
