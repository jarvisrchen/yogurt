//! Server-side transcript latency, from `transcript_tx.send` to the WS frame, must stay well under the 2s product budget.
//! Deepgram lag is verified by manual smoke.

use futures_util::StreamExt;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::Message;
use yogurt_server::{
    meetings,
    session::{load_or_create, SessionToken},
    storage::Storage,
    AppState, Mode,
};
use yogurt_stt::{Channel, TranscriptEvent};

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
        db,
        keys: Arc::new(yogurt_db::keys::MemoryKeyStore::default()),
        llm_override: Some(Arc::new(yogurt_server::__test_only_llm_mock::MockLlm)),
        meeting_repo,
        label_repo,
        app_events_tx: tokio::sync::broadcast::channel(64).0,
        detect: Default::default(),
        cloud_lock: Default::default(),
    };
    (state, token_str, tmp)
}

#[tokio::test(flavor = "multi_thread")]
async fn it_delivers_transcript_to_browser_well_under_2s() {
    let probe = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = probe.local_addr().unwrap();
    drop(probe);
    let (state, token, _tmp) = build_test_state(addr.port());
    let app = yogurt_server::__test_router(state.clone());
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });

    let m = state.meetings.create().await;
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
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while m.transcript_tx.receiver_count() == 0 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "WS handler never subscribed to transcript_tx within 5s"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }

    let t0 = Instant::now();
    m.transcript_tx
        .send(TranscriptEvent {
            ts_ms: 0,
            channel: Channel::Mic,
            text: "fast path".into(),
            is_final: true,
            confidence: None,
        })
        .unwrap();

    let msg = tokio::time::timeout(Duration::from_secs(2), ws.next())
        .await
        .expect("must arrive within 2s")
        .unwrap()
        .unwrap();
    let elapsed = t0.elapsed();
    match msg {
        Message::Text(t) => assert!(t.contains("fast path")),
        other => panic!("expected text: {other:?}"),
    }

    // 200ms is a CI-friendly ceiling; close to 2s would mean a real fan-out bug.
    assert!(
        elapsed < Duration::from_millis(200),
        "server-side lag was {elapsed:?}, expected < 200ms"
    );

    server.abort();
}
