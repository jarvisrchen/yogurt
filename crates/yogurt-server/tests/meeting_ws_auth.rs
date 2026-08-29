//! BL-01 regression: `/ws/meetings/{id}` requires Origin allowlist +
//! session-token, matching the Phase 0 `/ws` endpoint's auth contract.
//!
//! Four cases:
//!   (a) missing Origin     → 403
//!   (b) wrong Origin       → 403
//!   (c) missing/wrong token → 403
//!   (d) correct both       → 101 upgrade + transcript frame round-trip

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

struct TestSetup {
    addr: std::net::SocketAddr,
    token: String,
    state: AppState,
    _server: tokio::task::JoinHandle<()>,
    _tmp: tempfile::TempDir,
}

async fn spawn() -> TestSetup {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("yogurt-test.db");
    let token_path = tmp.path().join("session-token");
    let storage = Arc::new(Storage::init_at(&db_path).unwrap());
    let session_token = load_or_create(&token_path).unwrap();
    let token_str = session_token.as_str().to_string();
    let session: Arc<SessionToken> = Arc::new(session_token);

    let probe = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = probe.local_addr().unwrap();
    drop(probe);

    let (markdown_exporter, prompts) =
        yogurt_server::__test_only_aux_state(tmp.path().join("notes")).expect("build aux state");
    let db = yogurt_db::Db::open_in_memory().unwrap();
    let meeting_repo = Arc::new(yogurt_db::MeetingRepo::new(db.clone()));
    let label_repo = Arc::new(yogurt_db::LabelRepo::new(db.clone()));
    let state = AppState {
        mode: Mode::Release,
        storage,
        session,
        bind_port: addr.port(),
        meetings: meetings::Registry::new(),
        markdown_exporter,
        prompts,
        // Phase 5 (Plan 05-02): test wiring uses in-memory yogurt-db +
        // MemoryKeyStore so this test doesn't touch the real key file.
        db,
        keys: Arc::new(yogurt_db::keys::MemoryKeyStore::default()),
        // Phase 6 (Plan 06-01): test wiring uses MockLlm.
        llm_override: Some(Arc::new(yogurt_server::__test_only_llm_mock::MockLlm)),
        // Phase 7 (Plan 07-01): SQLite-backed Library directory.
        meeting_repo,
        label_repo,
        // Phase 8 (Plan 08-03): app-wide event broadcaster — unused
        // here but required by the AppState struct.
        app_events_tx: tokio::sync::broadcast::channel(64).0,
    };
    let app = yogurt_server::__test_router(state.clone());
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });
    // No readiness sleep needed: the listener is bound before the serve
    // task is spawned, so connections queue in the accept backlog.

    TestSetup {
        addr,
        token: token_str,
        state,
        _server: server,
        _tmp: tmp,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn it_rejects_meeting_ws_without_origin() {
    let s = spawn().await;
    let m = s.state.meetings.create().await;
    let url = format!(
        "ws://127.0.0.1:{}/ws/meetings/{}?token={}",
        s.addr.port(),
        m.id,
        s.token
    );

    // tungstenite always emits some Origin header by default; explicitly
    // strip it so the server sees "" and rejects.
    let mut req = url.into_client_request().expect("build req");
    req.headers_mut().remove("origin");
    // tungstenite's default Origin set by IntoClientRequest is the URL host;
    // by removing we force the server-side `headers.get("origin")` to None
    // which the handler maps to "" → not in allowlist → 403.

    let err = tokio_tungstenite::connect_async(req)
        .await
        .expect_err("expected 403 rejection (missing origin)");
    let msg = format!("{err}");
    assert!(
        msg.contains("403") || msg.to_lowercase().contains("forbidden"),
        "expected 403 in error; got: {msg}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn it_rejects_meeting_ws_with_wrong_origin() {
    let s = spawn().await;
    let m = s.state.meetings.create().await;
    let url = format!(
        "ws://127.0.0.1:{}/ws/meetings/{}?token={}",
        s.addr.port(),
        m.id,
        s.token
    );

    let mut req = url.into_client_request().expect("build req");
    req.headers_mut().insert(
        "origin",
        HeaderValue::from_static("http://evil.example.com"),
    );

    let err = tokio_tungstenite::connect_async(req)
        .await
        .expect_err("expected 403 rejection (wrong origin)");
    let msg = format!("{err}");
    assert!(
        msg.contains("403") || msg.to_lowercase().contains("forbidden"),
        "expected 403 in error; got: {msg}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn it_rejects_meeting_ws_without_token() {
    let s = spawn().await;
    let m = s.state.meetings.create().await;
    // No `?token=...` query.
    let url = format!("ws://127.0.0.1:{}/ws/meetings/{}", s.addr.port(), m.id);

    let mut req = url.into_client_request().expect("build req");
    req.headers_mut().insert(
        "origin",
        HeaderValue::from_str(&format!("http://127.0.0.1:{}", s.addr.port())).unwrap(),
    );

    let err = tokio_tungstenite::connect_async(req)
        .await
        .expect_err("expected 403 rejection (missing token)");
    let msg = format!("{err}");
    assert!(
        msg.contains("403") || msg.to_lowercase().contains("forbidden"),
        "expected 403 in error; got: {msg}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn it_rejects_meeting_ws_with_wrong_token() {
    let s = spawn().await;
    let m = s.state.meetings.create().await;
    let url = format!(
        "ws://127.0.0.1:{}/ws/meetings/{}?token=wrong-token-value-not-the-real-one",
        s.addr.port(),
        m.id
    );

    let mut req = url.into_client_request().expect("build req");
    req.headers_mut().insert(
        "origin",
        HeaderValue::from_str(&format!("http://127.0.0.1:{}", s.addr.port())).unwrap(),
    );

    let err = tokio_tungstenite::connect_async(req)
        .await
        .expect_err("expected 403 rejection (wrong token)");
    let msg = format!("{err}");
    assert!(
        msg.contains("403") || msg.to_lowercase().contains("forbidden"),
        "expected 403 in error; got: {msg}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn it_accepts_meeting_ws_with_correct_origin_and_token() {
    let s = spawn().await;
    let m = s.state.meetings.create().await;
    let url = format!(
        "ws://127.0.0.1:{}/ws/meetings/{}?token={}",
        s.addr.port(),
        m.id,
        s.token
    );

    let mut req = url.into_client_request().expect("build req");
    req.headers_mut().insert(
        "origin",
        HeaderValue::from_str(&format!("http://127.0.0.1:{}", s.addr.port())).unwrap(),
    );

    let (mut ws, response) = tokio_tungstenite::connect_async(req)
        .await
        .expect("ws upgrade should succeed");
    assert_eq!(response.status().as_u16(), 101);

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

    m.transcript_tx
        .send(yogurt_stt::TranscriptEvent {
            ts_ms: 42,
            channel: yogurt_stt::Channel::Mic,
            text: "auth happy path".into(),
            is_final: true,
        })
        .unwrap();

    let msg = tokio::time::timeout(Duration::from_secs(2), ws.next())
        .await
        .expect("frame within 2s")
        .expect("recv")
        .expect("ok");
    match msg {
        Message::Text(t) => assert!(t.contains("auth happy path")),
        other => panic!("expected text frame, got {other:?}"),
    }
}
