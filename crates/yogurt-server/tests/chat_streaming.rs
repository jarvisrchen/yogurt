//! Phase 6 (Plan 06-01) — chat REST + WS streaming integration tests.
//!
//! Exercises the full happy path against a deterministic `MockChunksLlm`:
//! POST returns a ULID `message_id`, and a WS subscriber sees an ordered
//! sequence of `chat_chunk` frames whose concatenated `delta`s equal the
//! mock's pre-canned text.

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::Message;
use yogurt_server::test_support::{run_with_mock_llm, seed_meeting};

#[tokio::test(flavor = "multi_thread")]
async fn it_returns_message_id_on_post_chat() {
    let (srv, server_task) = run_with_mock_llm(&["one"]).await.expect("boot server");
    let meeting_id = seed_meeting(&srv.state).await;

    let url = format!(
        "http://{}/api/meetings/{}/chat?token={}",
        srv.addr, meeting_id, srv.token
    );
    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .json(&serde_json::json!({ "content": "hello" }))
        .send()
        .await
        .expect("post chat");
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.expect("json body");
    let message_id = body["message_id"].as_str().expect("message_id string");
    assert_eq!(
        message_id.len(),
        26,
        "ULID is 26 chars, got: {message_id:?}"
    );

    server_task.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn it_streams_chat_chunks_in_order_over_ws() {
    let (srv, server_task) = run_with_mock_llm(&["hello ", "world", "."])
        .await
        .expect("boot server");
    let meeting_id = seed_meeting(&srv.state).await;

    // Connect a WS client first so we don't miss the chunks emitted by the
    // detached spawn_stream task.
    let ws_url = format!(
        "ws://{}/ws/meetings/{}?token={}",
        srv.addr, meeting_id, srv.token
    );
    let mut req = ws_url.into_client_request().expect("build req");
    req.headers_mut().insert(
        "origin",
        HeaderValue::from_str(&format!("http://127.0.0.1:{}", srv.addr.port())).unwrap(),
    );
    let (ws_stream, _) = tokio_tungstenite::connect_async(req)
        .await
        .expect("ws connect");
    let (mut ws_write, mut ws_read) = ws_stream.split();

    // Give the WS handler a moment to subscribe to events_tx before we
    // post and trigger the broadcast.
    tokio::time::sleep(Duration::from_millis(75)).await;

    // POST the chat request.
    let post_url = format!(
        "http://{}/api/meetings/{}/chat?token={}",
        srv.addr, meeting_id, srv.token
    );
    let client = reqwest::Client::new();
    let resp = client
        .post(&post_url)
        .json(&serde_json::json!({ "content": "say something" }))
        .send()
        .await
        .expect("post chat");
    let body: serde_json::Value = resp.json().await.expect("json body");
    let expected_message_id = body["message_id"]
        .as_str()
        .expect("message_id string")
        .to_string();

    // Collect chat_chunk events for 3s (mock is instantaneous; this is
    // headroom for slow CI).
    let mut accumulated = String::new();
    let mut saw_done = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        let next = tokio::time::timeout(remaining, ws_read.next()).await;
        let frame = match next {
            Ok(Some(Ok(f))) => f,
            _ => break,
        };
        let text = match frame {
            Message::Text(t) => t,
            // tungstenite ping/pong frames are auto-handled; ignore anything
            // else that comes through.
            _ => continue,
        };
        let value: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if value["type"] != "chat_chunk" {
            continue;
        }
        assert_eq!(
            value["message_id"], expected_message_id,
            "all chunks must share the same message_id"
        );
        if let Some(delta) = value["delta"].as_str() {
            accumulated.push_str(delta);
        }
        if value["done"].as_bool().unwrap_or(false) {
            saw_done = true;
            break;
        }
    }

    // Politely close the WS.
    let _ = ws_write.send(Message::Close(None)).await;

    assert!(saw_done, "expected at least one chunk with done=true");
    assert_eq!(
        accumulated, "hello world.",
        "concatenated deltas must equal the mock's content"
    );

    server_task.abort();
}
