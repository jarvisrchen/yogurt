//! End-to-end WS test that doesn't depend on Deepgram: directly publish into
//! the meeting's transcript broadcast and assert the frame lands on a
//! connected WS client in the expected JSON envelope (PRD §10 / CONTEXT D-10).

use futures_util::StreamExt;
use std::sync::Arc;
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;
use yogurt_server::{
    meetings,
    session::{load_or_create, SessionToken},
    storage::Storage,
    AppState, Mode,
};

/// Construct a full `AppState` rooted in a tempdir so the test never touches
/// the developer's real `~/.yogurt/`. Mirrors what `run_with_config` does
/// internally, but the test holds the registry handle so it can `send`
/// transcript events directly.
fn build_test_state(bind_port: u16) -> (AppState, tempfile::TempDir) {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("yogurt-test.db");
    let token_path = tmp.path().join("session-token");
    let storage = Arc::new(Storage::init_at(&db_path).unwrap());
    let session: Arc<SessionToken> = Arc::new(load_or_create(&token_path).unwrap());
    let state = AppState {
        mode: Mode::Release,
        storage,
        session,
        bind_port,
        meetings: meetings::Registry::new(),
    };
    (state, tmp)
}

#[tokio::test(flavor = "multi_thread")]
async fn it_fans_transcript_events_to_ws_clients() {
    let addr: std::net::SocketAddr = "127.0.0.1:17892".parse().unwrap();
    let (state, _tmp) = build_test_state(addr.port());
    let app = yogurt_server::__test_router(state.clone());
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });

    // Create a meeting via the registry directly (skip REST for this test).
    let m = state.meetings.create().await;

    // Connect a WS client.
    let url = format!("ws://127.0.0.1:17892/ws/meetings/{}", m.id);
    let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.unwrap();

    // Wait briefly for the handler to subscribe before publishing.
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Publish a transcript event.
    m.transcript_tx
        .send(yogurt_stt::TranscriptEvent {
            ts_ms: 11_020,
            channel: yogurt_stt::Channel::Mic,
            text: "hello from the test".into(),
            is_final: true,
        })
        .unwrap();

    // Read the frame.
    let msg = tokio::time::timeout(Duration::from_secs(2), ws.next())
        .await
        .expect("frame within 2s")
        .unwrap()
        .unwrap();
    let text = match msg {
        Message::Text(t) => t,
        other => panic!("expected text frame, got {other:?}"),
    };
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["type"], "transcript");
    assert_eq!(v["payload"]["text"], "hello from the test");
    assert_eq!(v["payload"]["channel"], "mic");
    assert_eq!(v["payload"]["ts_ms"], 11_020);

    server.abort();
}
