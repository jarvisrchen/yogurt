//! Chat REST + WS streaming against a deterministic `MockChunksLlm`.

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
    let meeting = srv
        .state
        .meetings
        .get(&meeting_id)
        .await
        .expect("meeting in registry");
    let baseline_subscribers = meeting.events_tx.receiver_count();

    // Connect first so chunks from the detached stream task are not missed.
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

    // Wait for the WS handler to subscribe before posting.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while meeting.events_tx.receiver_count() <= baseline_subscribers {
        assert!(
            tokio::time::Instant::now() < deadline,
            "WS handler never subscribed to events_tx within 5s"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }

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

    let _ = ws_write.send(Message::Close(None)).await;

    assert!(saw_done, "expected at least one chunk with done=true");
    assert_eq!(
        accumulated, "hello world.",
        "concatenated deltas must equal the mock's content"
    );

    server_task.abort();
}

/// A meeting that exists only in SQLite (registry wiped by a restart) must still get a working WS, not a 4404 close.
#[tokio::test(flavor = "multi_thread")]
async fn ws_attaches_to_sqlite_only_meeting_and_streams_chat() {
    let (srv, server_task) = run_with_mock_llm(&["resurrected"])
        .await
        .expect("boot server");

    // In the repo but not the in-memory registry, as after a restart.
    let meeting_id = uuid::Uuid::now_v7();
    let id_str = meeting_id.to_string();
    {
        let writer = srv.state.storage.writer();
        let conn = writer.lock().expect("writer lock");
        conn.execute(
            r#"INSERT OR IGNORE INTO meetings (id, title, started_at, transcript_json)
                  VALUES (?1, ?2, ?3, ?4)"#,
            rusqlite::params![id_str, "restart survivor", 0i64, "[]"],
        )
        .expect("seed storage row");
    }
    srv.state
        .meeting_repo
        .create(yogurt_db::NewMeeting {
            title: "restart survivor".into(),
            started_at_unix_ms: Some(0),
            id: Some(id_str.clone()),
        })
        .expect("seed repo row");
    assert!(
        srv.state.meetings.get(&meeting_id).await.is_none(),
        "precondition: registry must not know this meeting"
    );

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
    let (_ws_write, mut ws_read) = ws_stream.split();

    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    let meeting = loop {
        if let Some(m) = srv.state.meetings.get(&meeting_id).await {
            if m.events_tx.receiver_count() > 0 {
                break m;
            }
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "WS handler never hydrated + subscribed within 5s"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    };
    drop(meeting);

    let post_url = format!(
        "http://{}/api/meetings/{}/chat?token={}",
        srv.addr, meeting_id, srv.token
    );
    let client = reqwest::Client::new();
    let resp = client
        .post(&post_url)
        .json(&serde_json::json!({ "content": "are you alive?" }))
        .send()
        .await
        .expect("post chat");
    assert_eq!(resp.status(), 200);

    let mut accumulated = String::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while tokio::time::Instant::now() < deadline {
        let next = tokio::time::timeout(Duration::from_millis(300), ws_read.next()).await;
        let Ok(Some(Ok(msg))) = next else { continue };
        if let tokio_tungstenite::tungstenite::Message::Text(text) = msg {
            let frame: serde_json::Value = serde_json::from_str(&text).expect("json frame");
            if frame["type"] == "chat_chunk" {
                accumulated.push_str(frame["delta"].as_str().unwrap_or(""));
                if frame["done"] == true {
                    break;
                }
            }
        }
    }
    assert_eq!(accumulated, "resurrected");

    server_task.abort();
}
