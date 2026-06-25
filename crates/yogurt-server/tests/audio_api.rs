//! Integration tests for the `/api/audio/*` REST endpoints.
//!
//! Both endpoints are shape-asserting only; they do not assert specific
//! device names or permission states (those are hardware-dependent and
//! would make the tests environment-fragile). The shape contract is what
//! Phase 5 settings UI and Phase 7 onboarding consume.

use std::time::Duration;

#[tokio::test]
async fn it_lists_audio_devices() {
    let addr = "127.0.0.1:17890".parse().unwrap();
    let mode = yogurt_server::Mode::Release;

    let handle = tokio::spawn(async move { yogurt_server::run(addr, mode).await });
    tokio::time::sleep(Duration::from_millis(200)).await;

    let body = reqwest::get("http://127.0.0.1:17890/api/audio/devices")
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

    handle.abort();
}

#[tokio::test]
async fn it_reports_permission_status() {
    let addr = "127.0.0.1:17891".parse().unwrap();
    let mode = yogurt_server::Mode::Release;

    let handle = tokio::spawn(async move { yogurt_server::run(addr, mode).await });
    tokio::time::sleep(Duration::from_millis(200)).await;

    let body = reqwest::get("http://127.0.0.1:17891/api/audio/permission")
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

    handle.abort();
}
