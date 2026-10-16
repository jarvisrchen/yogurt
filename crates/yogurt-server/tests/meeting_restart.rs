//! Regression: stopping then restarting the same meeting must not overwrite `started_at` or leave a stale `ended_at`.
//!
//! `POST /api/meetings/:id/start` cannot run here: it opens real ScreenCaptureKit and cpal capture, which needs Screen Recording permission.
//! These tests drive `start_stamp_patch` (re-exported as `__test_only_start_stamp`), the pure function `start_meeting` calls, against a real `MeetingRepo` row.

use yogurt_server::__test_only_start_stamp::start_stamp_patch;
use yogurt_server::test_support::{run_with_mock_llm, seed_meeting};

#[tokio::test(flavor = "multi_thread")]
async fn restart_preserves_started_at_and_clears_ended_at() {
    let (server, handle) = run_with_mock_llm(&[]).await.expect("boot test server");
    let id = seed_meeting(&server.state).await;
    let id_str = id.to_string();

    let original_started_at = 1_000_000_i64;
    let stale_ended_at = 2_000_000_i64;
    server
        .state
        .meeting_repo
        .patch(
            &id_str,
            yogurt_db::MeetingPatch {
                started_at: Some(original_started_at),
                ended_at: Some(Some(stale_ended_at)),
                ..Default::default()
            },
        )
        .expect("seed session-1-completed state");

    let before = server
        .state
        .meeting_repo
        .get(&id_str)
        .expect("get row")
        .expect("row exists");
    let patch = start_stamp_patch(Some(&before));
    server
        .state
        .meeting_repo
        .patch(&id_str, patch)
        .expect("apply start_stamp_patch");

    let client = reqwest::Client::new();
    let after: serde_json::Value = client
        .get(format!("http://{}/api/meetings/{id_str}", server.addr))
        .bearer_auth(&server.token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_eq!(
        after["started_at"].as_i64(),
        Some(original_started_at),
        "restart must not overwrite the original session's started_at"
    );
    assert!(
        after["ended_at"].is_null(),
        "restart must clear the stale ended_at so the next stop stamps fresh"
    );

    handle.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn genuine_first_start_stamps_started_at() {
    let (server, handle) = run_with_mock_llm(&[]).await.expect("boot test server");
    let id = seed_meeting(&server.state).await;
    let id_str = id.to_string();

    let before = server
        .state
        .meeting_repo
        .get(&id_str)
        .expect("get row")
        .expect("row exists");
    assert_eq!(
        before.started_at, 0,
        "seed_meeting seeds the schema's unstarted sentinel"
    );
    assert!(before.ended_at.is_none());

    let patch = start_stamp_patch(Some(&before));
    let after = server
        .state
        .meeting_repo
        .patch(&id_str, patch)
        .expect("apply start_stamp_patch");

    assert!(
        after.started_at > 0,
        "a genuine first start must stamp a real started_at"
    );
    assert!(after.ended_at.is_none());

    handle.abort();
}
