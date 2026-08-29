//! End-to-end WS test that doesn't depend on Deepgram: directly publish into
//! the meeting's transcript broadcast and assert the frame lands on a
//! connected WS client in the expected JSON envelope (PRD §10 / CONTEXT D-10).
//!
//! BL-01: the meeting WS endpoint now requires Origin + session-token, so the
//! happy-path test threads both through the upgrade request. The auth
//! rejection matrix (missing/wrong origin, missing/wrong token) lives in
//! `meeting_ws_auth.rs`.

use futures_util::StreamExt;
use std::sync::Arc;
use std::time::Duration;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
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
/// transcript events directly. Returns the bound port + the session token so
/// the test can present matching `Origin: http://127.0.0.1:<port>` and
/// `?token=<token>` on the upgrade request (BL-01).
fn build_test_state(bind_port: u16) -> (AppState, String, tempfile::TempDir) {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("yogurt-test.db");
    let token_path = tmp.path().join("session-token");
    let storage = Arc::new(Storage::init_at(&db_path).unwrap());
    let session_token = load_or_create(&token_path).unwrap();
    let token_str = session_token.as_str().to_string();
    let session: Arc<SessionToken> = Arc::new(session_token);
    let (markdown_exporter, prompts) =
        yogurt_server::__test_only_aux_state(tmp.path().join("notes")).expect("build aux state");
    let db = yogurt_db::Db::open_in_memory().unwrap();
    let meeting_repo = Arc::new(yogurt_db::MeetingRepo::new(db.clone()));
    let label_repo = Arc::new(yogurt_db::LabelRepo::new(db.clone()));
    let state = AppState {
        mode: Mode::Release,
        storage,
        session,
        bind_port,
        meetings: meetings::Registry::new(),
        markdown_exporter,
        prompts,
        // Phase 5 (Plan 05-02): test wiring uses in-memory yogurt-db +
        // MemoryKeyStore so this test doesn't touch the real Keychain.
        db,
        keys: Arc::new(yogurt_db::keychain::MemoryKeyStore::default()),
        // Phase 6 (Plan 06-01): test wiring uses the deterministic mock LLM
        // so chat-spawn tasks (if reached) don't hit the network.
        llm_override: Some(Arc::new(yogurt_server::__test_only_llm_mock::MockLlm)),
        // Phase 7 (Plan 07-01): SQLite-backed Library directory.
        meeting_repo,
        label_repo,
        // Phase 8 (Plan 08-03): app-wide event broadcaster for
        // STT model download progress. Unused in this test but
        // required by the AppState struct.
        app_events_tx: tokio::sync::broadcast::channel(64).0,
    };
    (state, token_str, tmp)
}

#[tokio::test(flavor = "multi_thread")]
async fn it_fans_transcript_events_to_ws_clients() {
    // Bind on an ephemeral port (IN-06) — fixed ports race under `cargo
    // nextest`. We bind once with the kernel-assigned port, learn the port,
    // drop the listener, then pass the same addr to the server.
    let probe = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = probe.local_addr().unwrap();
    drop(probe);
    let (state, token, _tmp) = build_test_state(addr.port());
    let app = yogurt_server::__test_router(state.clone());
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });

    // Create a meeting via the registry directly (skip REST for this test).
    let m = state.meetings.create().await;

    // Connect a WS client with the correct Origin + session-token (BL-01).
    let url = format!(
        "ws://127.0.0.1:{}/ws/meetings/{}?token={}",
        addr.port(),
        m.id,
        token
    );
    let mut req = url.into_client_request().expect("build req");
    req.headers_mut().insert(
        "origin",
        HeaderValue::from_str(&format!("http://127.0.0.1:{}", addr.port())).unwrap(),
    );
    let (mut ws, _) = tokio_tungstenite::connect_async(req).await.unwrap();

    // Deterministic subscribe signal: poll the broadcast's receiver count
    // instead of a fixed sleep (flake source on slow CI).
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while m.transcript_tx.receiver_count() == 0 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "WS handler never subscribed to transcript_tx within 5s"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }

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
