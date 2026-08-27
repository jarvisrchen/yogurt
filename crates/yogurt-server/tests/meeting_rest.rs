//! REST lifecycle tests for `/api/meetings*` (Phase 3 / Plan 03-02 Task 2).
//!
//! - `it_creates_a_meeting_and_returns_an_id` — happy path; asserts UUID
//!   parses and `created_at_ms > 0`.
//! - `it_rejects_start_without_api_key` — D-07 contract; with the env var
//!   unset, `POST /start` returns 400 with an actionable message pointing
//!   the user at Settings (the message no longer names the raw env var —
//!   see `meetings::select_stt`'s cloud branch).
//! - `it_rejects_meeting_endpoints_without_token` — WR-06 regression: the
//!   meeting REST endpoints now require a session token (matching the
//!   WS endpoint's auth contract).
//!
//! IN-06: tests bind to ephemeral 127.0.0.1:0 instead of fixed ports so they
//! cannot collide under `cargo nextest --threads`.

use std::time::Duration;

use yogurt_server::{run_with_config, Mode, RunConfig};

/// Spawn a fresh server bound to an ephemeral port with temp DB + temp
/// session token (so the test never touches the developer's real
/// `~/.yogurt/`). Returns the bound addr, the seeded session token (so
/// tests can authenticate), and the tempdir handle (kept alive for the
/// duration of the test).
async fn spawn_server() -> (
    std::net::SocketAddr,
    String,
    tokio::task::JoinHandle<()>,
    tempfile::TempDir,
) {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("yogurt-test.db");
    let token_path = tmp.path().join("session-token");

    // Pre-create the token so the test can send it back on requests.
    let token = yogurt_server::session::load_or_create(&token_path)
        .expect("seed session token")
        .as_str()
        .to_string();

    let probe = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral");
    let addr = probe.local_addr().unwrap();
    drop(probe);

    let cfg = RunConfig {
        addr,
        mode: Mode::Release,
        db_path: Some(db_path),
        session_token_path: Some(token_path),
        // Phase 4 (Plan 04-03): sandbox the per-meeting notes dir.
        notes_dir: Some(tmp.path().join("notes")),
        // Phase 5 (SET-12): tempdir-isolate the new yogurt-db so parallel
        // tests do not deadlock on the real user DB's WAL lock.
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
    // Phase 7 (Plan 07-01): the wire shape moved from
    // `{id, created_at_ms}` (Phase 3 in-memory) to the full
    // `yogurt_db::Meeting` projection. `created_at` is now an ISO 8601
    // UTC string; existence + non-empty is the smoke check.
    assert!(
        body["created_at"]
            .as_str()
            .map(|s| !s.is_empty())
            .unwrap_or(false),
        "created_at is a non-empty ISO 8601 string"
    );
    assert_eq!(body["title"].as_str().unwrap(), "Untitled meeting");

    handle.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn it_rejects_start_without_api_key() {
    // Ensure the env var is NOT set for this test. SAFETY: tests in the
    // same binary share the process env; this test pair is sequential.
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
    // Regression note: `select_stt`'s cloud-branch error (meetings.rs) was
    // reworded to point users at Settings instead of the raw env var name
    // — the message no longer literally contains "YOGURT_DEEPGRAM_API_KEY".
    // Assert on the current, actionable contract instead (mirrors
    // `meetings::tests::rejects_cloud_without_key`).
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

/// WR-06 regression: hitting the meeting REST endpoints WITHOUT a token
/// must yield 403 (matching `/api/audio/*` behavior).
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

/// WR-06: bootstrap endpoint `GET /api/session-token` returns the token
/// when called from an allowed Origin; rejects otherwise.
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

/// REGRESSION: browsers omit `Origin` on same-origin GET (per Fetch spec),
/// so the bootstrap MUST accept empty Origin + `Sec-Fetch-Site: same-origin`.
/// Without this, the SPA cannot fetch its session token and the app never
/// loads. The original Phase 0 test above set Origin manually and missed
/// this — that's why the bug shipped through autonomous verification.
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

/// REGRESSION: cross-site Sec-Fetch-Site must still 403 — an attacker
/// iframe gets `Sec-Fetch-Site: cross-site` (unforgeable) and no Origin
/// when fetching with credentials, and must not be able to read the token.
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
