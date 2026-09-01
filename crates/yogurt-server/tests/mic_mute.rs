//! REST contract tests for `POST /api/meetings/:id/mic-muted` (AUD-6).
//!
//! Mirrors `audio_device_switch.rs`: none of these require real audio
//! hardware or Screen Recording permission — they exercise only the
//! "meeting not found" / "not recording" / "no token" branches, which is as
//! far as this environment can safely exercise `Registry::start`.

use std::time::Duration;

use yogurt_server::{run_with_config, Mode, RunConfig};

/// Spawn a fresh server bound to an ephemeral port with temp DB + temp
/// session token (so the test never touches the developer's real
/// `~/.yogurt/`). Copied verbatim (pattern) from `audio_device_switch.rs`.
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
async fn it_returns_404_for_unknown_meeting() {
    let (addr, token, handle, _tmp) = spawn_server().await;

    let random_id = uuid::Uuid::now_v7();
    let resp = reqwest::Client::new()
        .post(format!("http://{addr}/api/meetings/{random_id}/mic-muted"))
        .bearer_auth(&token)
        .json(&serde_json::json!({ "muted": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);

    handle.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn it_returns_409_when_meeting_is_not_recording() {
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

    // Never call /start — the meeting exists but is not recording.
    let resp = client
        .post(format!("http://{addr}/api/meetings/{id}/mic-muted"))
        .bearer_auth(&token)
        .json(&serde_json::json!({ "muted": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 409);

    handle.abort();
}

/// Mirrors `it_rejects_unauthenticated_switch_requests` in
/// `audio_device_switch.rs` — no token must yield 403.
#[tokio::test(flavor = "multi_thread")]
async fn it_rejects_unauthenticated_mute_requests() {
    let (addr, _token, handle, _tmp) = spawn_server().await;

    let random_id = uuid::Uuid::now_v7();
    let resp = reqwest::Client::new()
        .post(format!("http://{addr}/api/meetings/{random_id}/mic-muted"))
        .json(&serde_json::json!({ "muted": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403);

    handle.abort();
}

/// `GET /api/meetings/active` returns `null` when nothing is recording —
/// AUD-6's `mic_muted` field only appears inside a non-null body, so there
/// is nothing to assert about it here beyond the endpoint staying a clean
/// `null`, not erroring, once `mic_muted` reads through `Registry::get`.
#[tokio::test(flavor = "multi_thread")]
async fn active_recording_stays_null_when_nothing_is_recording() {
    let (addr, token, handle, _tmp) = spawn_server().await;

    let resp = reqwest::Client::new()
        .get(format!("http://{addr}/api/meetings/active"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.json::<serde_json::Value>().await.unwrap(),
        serde_json::Value::Null
    );

    handle.abort();
}
