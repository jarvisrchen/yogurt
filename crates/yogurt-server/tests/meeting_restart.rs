//! Regression tests for the "stop, then restart the SAME meeting"
//! data-loss bug: a second recording session used to overwrite
//! `started_at` (routes.rs's `start_meeting` handler), leave a stale
//! `ended_at` behind, and (separately, covered in `meetings.rs`'s own test
//! module) blow away the first session's `transcript_json`.
//!
//! `POST /api/meetings/:id/start` cannot be driven end-to-end here: it
//! opens real ScreenCaptureKit + cpal audio capture on a dedicated thread,
//! which has no fake/mock mode and cannot succeed without Screen Recording
//! permission — unavailable in this sandboxed test runner. That's also why
//! the pre-existing `tests/meeting_rest.rs::it_rejects_start_without_api_key`
//! only ever exercises `/start`'s FAILURE path.
//!
//! Instead, these tests drive `start_stamp_patch` — the exact pure
//! decision function `start_meeting`'s success branch calls to compute the
//! patch — re-exported for tests via `__test_only_start_stamp`, against a
//! real `MeetingRepo` row reached through the real REST surface. The only
//! thing not under test is the literal HTTP round trip through the
//! audio-gated handler body; the row state, the patch computation, and the
//! tri-state `ended_at` semantics are all real.

use yogurt_server::__test_only_start_stamp::start_stamp_patch;
use yogurt_server::test_support::{run_with_mock_llm, seed_meeting};

#[tokio::test(flavor = "multi_thread")]
async fn restart_preserves_started_at_and_clears_ended_at() {
    let (server, handle) = run_with_mock_llm(&[]).await.expect("boot test server");
    let id = seed_meeting(&server.state).await;
    let id_str = id.to_string();

    // Simulate "session 1 completed": a real first start stamped
    // `started_at`, a real stop stamped `ended_at`.
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

    // Exactly what `start_meeting`'s Ok(_) branch does: read the pre-start
    // row, compute the patch, apply it.
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

    // Assert over the real REST surface, not just the repo directly.
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
