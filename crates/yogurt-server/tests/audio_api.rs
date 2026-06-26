//! Integration tests for the `/api/audio/*` REST endpoints.
//!
//! Both endpoints are shape-asserting only; they do not assert specific
//! device names or permission states (those are hardware-dependent and
//! would make the tests environment-fragile). The shape contract is what
//! Phase 5 settings UI and Phase 7 onboarding consume.
//!
//! WR-09: tests use `run_with_config` + `tempfile::TempDir`-backed paths
//! (matching the Phase 0 pattern) so they do not clobber the developer's
//! real `~/.yogurt/` DB or session-token. They also poll the bound port
//! until reachable instead of sleeping a fixed duration, so they do not
//! flake on slow CI boxes.
//!
//! WR-08: both endpoints now require a session token. Tests pre-generate
//! the token via `yogurt_server::session::load_or_create` and present it
//! to the server via `Authorization: Bearer <token>`.

use std::net::SocketAddr;
use std::time::Duration;
use yogurt_server::{Mode, RunConfig};

/// Bring up a server on an ephemeral port, write a tempdir-backed
/// session-token + DB file, and return `(addr, token, _tempdir)`. Holding
/// the returned `TempDir` keeps the per-test scratch directory alive for
/// the body of the test.
async fn spawn_server() -> (SocketAddr, String, tempfile::TempDir) {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let token_path = tmp.path().join("session-token");
    let db_path = tmp.path().join("yogurt.sqlite");

    // Pre-create the token so we can read it back and send it on requests.
    let token = yogurt_server::session::load_or_create(&token_path)
        .expect("seed session token in tempdir")
        .0;

    // Bind to port 0 → kernel picks an ephemeral free port. We then learn
    // the actual bound port by listening ourselves first, then dropping the
    // listener and letting the server re-bind. There is a race window, but
    // the polling loop below tolerates it.
    let probe = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral");
    let addr: SocketAddr = probe.local_addr().expect("local_addr");
    drop(probe);

    let cfg = RunConfig {
        addr,
        mode: Mode::Release,
        db_path: Some(db_path),
        session_token_path: Some(token_path),
    };
    tokio::spawn(async move {
        let _ = yogurt_server::run_with_config(cfg).await;
    });

    // Poll the health endpoint until the server is up (200) or we time out.
    // 50 × 20ms = 1s ceiling — fast on a healthy box, generous on a slow
    // CI runner. /api/health is unauthenticated so it works without a token.
    for _ in 0..50 {
        if reqwest::get(format!("http://{addr}/api/health"))
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
        {
            return (addr, token, tmp);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("server did not become reachable within 1 second");
}

#[tokio::test]
async fn it_lists_audio_devices() {
    let (addr, token, _tmp) = spawn_server().await;

    let client = reqwest::Client::new();
    let body = client
        .get(format!("http://{addr}/api/audio/devices"))
        .bearer_auth(&token)
        .send()
        .await
        .expect("server reachable")
        .json::<serde_json::Value>()
        .await
        .expect("valid JSON");

    // Response must be a JSON array.
    let arr = body.as_array().expect("response is a JSON array");

    // If we're on a host with any input devices (typical Mac dev box),
    // each entry must have the `name` (string) and `is_default` (bool)
    // fields the Phase 5 settings UI dropdown will key on. CI runners
    // without audio devices may return an empty array — that's fine.
    for entry in arr {
        assert!(
            entry.get("name").and_then(|v| v.as_str()).is_some(),
            "device entry missing `name` string: {entry:?}"
        );
        assert!(
            entry.get("is_default").and_then(|v| v.as_bool()).is_some(),
            "device entry missing `is_default` bool: {entry:?}"
        );
        // sample_rate may be null when cpal can't report one — accept
        // either a u64 or a null, but the key must be present in the JSON.
        assert!(
            entry.get("sample_rate").is_some(),
            "device entry missing `sample_rate` key: {entry:?}"
        );
    }
}

#[tokio::test]
async fn it_reports_permission_status() {
    let (addr, token, _tmp) = spawn_server().await;

    let client = reqwest::Client::new();
    let body = client
        .get(format!("http://{addr}/api/audio/permission"))
        .bearer_auth(&token)
        .send()
        .await
        .expect("server reachable")
        .json::<serde_json::Value>()
        .await
        .expect("valid JSON");

    let status = body
        .get("status")
        .and_then(|v| v.as_str())
        .expect("response has `status` string field");

    assert!(
        matches!(status, "granted" | "denied" | "not_required"),
        "status must be one of granted|denied|not_required, got {status:?}"
    );
}

/// WR-08 regression: hitting `/api/audio/*` without a token must yield 403,
/// not the response body. Mirrors the WS endpoint's behaviour.
#[tokio::test]
async fn it_rejects_unauthenticated_audio_calls() {
    let (addr, _token, _tmp) = spawn_server().await;

    let resp = reqwest::get(format!("http://{addr}/api/audio/devices"))
        .await
        .expect("server reachable");
    assert_eq!(resp.status(), reqwest::StatusCode::FORBIDDEN);

    let resp = reqwest::get(format!("http://{addr}/api/audio/permission"))
        .await
        .expect("server reachable");
    assert_eq!(resp.status(), reqwest::StatusCode::FORBIDDEN);
}

/// WR-08 regression: a wrong token also yields 403 (not just missing).
#[tokio::test]
async fn it_rejects_wrong_token_audio_calls() {
    let (addr, _token, _tmp) = spawn_server().await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("http://{addr}/api/audio/devices"))
        .bearer_auth("wrong-token-value-that-does-not-match-the-real-one")
        .send()
        .await
        .expect("server reachable");
    assert_eq!(resp.status(), reqwest::StatusCode::FORBIDDEN);
}

/// WR-08: the `?token=<token>` query-string fallback matches the WS
/// endpoint's convention. Useful when a caller cannot easily set custom
/// headers (e.g. an image-preload exploit cannot, which is the point).
#[tokio::test]
async fn it_accepts_query_string_token_audio_calls() {
    let (addr, token, _tmp) = spawn_server().await;

    let resp = reqwest::get(format!("http://{addr}/api/audio/permission?token={token}"))
        .await
        .expect("server reachable");
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
}
