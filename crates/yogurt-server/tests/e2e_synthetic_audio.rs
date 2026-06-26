//! Acceptance test for PRD §5.2 / TRANS-08: "transcript appears with < 2s lag".
//!
//! We test the SERVER side of that budget — from `transcript_tx.send(...)` to
//! the WS client receiving the frame — must be well under 2s. Network lag to
//! Deepgram + Deepgram's own processing are budgeted separately and verified
//! by manual smoke against the real API (superpowers Task 3.10 Step 3).
//!
//! Asserts < 200ms (CONTEXT D-19) — the part we own. Anything close to 2s
//! here would point at a real bug in the broadcast → WS serialize path.

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
    let state = AppState {
        mode: Mode::Release,
        storage,
        session,
        bind_port,
        meetings: meetings::Registry::new(),
    };
    (state, token_str, tmp)
}

#[tokio::test(flavor = "multi_thread")]
async fn it_delivers_transcript_to_browser_well_under_2s() {
    // IN-06: bind ephemeral port instead of hard-coded 17893.
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
    // BL-01: this WS endpoint now requires Origin + session-token.
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
    tokio::time::sleep(Duration::from_millis(50)).await;

    let t0 = Instant::now();
    m.transcript_tx
        .send(TranscriptEvent {
            ts_ms: 0,
            channel: Channel::Mic,
            text: "fast path".into(),
            is_final: true,
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

    // The actual budget we own (sender → ws frame) should be tens of
    // milliseconds. 200ms here is a generous CI-friendly ceiling — anything
    // close to 2s would point at a real bug in the fan-out path.
    assert!(
        elapsed < Duration::from_millis(200),
        "server-side lag was {elapsed:?}, expected < 200ms"
    );

    server.abort();
}
