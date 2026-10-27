//! AUD-14: when the system-audio source goes quiet the mic must keep
//! flowing, the meeting must stay up, and the watchdog's status notices must
//! reach the meeting's WS client as `system_audio_status` frames.

use futures_util::StreamExt;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::Message;
use yogurt_audio::{Channel, Frame, SystemStatus};
use yogurt_server::{
    meetings,
    session::{load_or_create, SessionToken},
    storage::Storage,
    AppState, Mode,
};

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
        detect: Default::default(),
        cloud_lock: Default::default(),
        import: Default::default(),
        cloud_push: Default::default(),
    };
    (state, token_str, tmp)
}

type Ws =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// Next WS frame whose `type` matches, skipping the others (mic `audio_level`).
async fn next_of_type(ws: &mut Ws, ty: &str) -> serde_json::Value {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let Message::Text(t) = ws.next().await.unwrap().unwrap() else {
                continue;
            };
            let v: serde_json::Value = serde_json::from_str(&t).unwrap();
            if v["type"] == ty {
                return v;
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("no {ty} frame within 5s"))
}

#[tokio::test(flavor = "multi_thread")]
async fn system_stall_reaches_ws_while_mic_keeps_flowing() {
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
    let mut req = url.into_client_request().unwrap();
    req.headers_mut().insert(
        "origin",
        HeaderValue::from_str(&format!("http://127.0.0.1:{}", addr.port())).unwrap(),
    );
    let (mut ws, _) = tokio_tungstenite::connect_async(req).await.unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while m.events_tx.receiver_count() == 0 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "WS never subscribed"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }

    // Synthetic mic at the real 20 ms cadence. The system sender stays open
    // but never sends, which is exactly what the bug looked like downstream.
    let (mic_tx, mic_rx) = broadcast::channel::<Frame>(64);
    let (_sys_tx, sys_rx) = broadcast::channel::<Frame>(64);
    let (status_tx, status_rx) = broadcast::channel::<SystemStatus>(8);
    let (audio_tx, _audio_rx) = broadcast::channel(64);
    let mic = tokio::spawn(async move {
        let mut t = tokio::time::interval(Duration::from_millis(20));
        let mut i = 0u64;
        loop {
            t.tick().await;
            let _ = mic_tx.send(Frame {
                channel: Channel::Mic,
                samples: vec![8000; 320],
                monotonic_micros: i * 20_000,
            });
            i += 1;
        }
    });
    let relay = tokio::spawn(meetings::relay_system_status(
        status_rx,
        m.events_tx.clone(),
    ));
    let pump = tokio::spawn(meetings::pump_audio_adapter(
        mic_rx,
        sys_rx,
        audio_tx,
        m.events_tx.clone(),
        Default::default(),
    ));

    assert_eq!(next_of_type(&mut ws, "audio_level").await["channel"], "mic");

    status_tx.send(SystemStatus::Stalled).unwrap();
    let stalled = next_of_type(&mut ws, "system_audio_status").await;
    assert_eq!(stalled["state"], "stalled");
    // The mic is still being pumped after the notice.
    assert_eq!(next_of_type(&mut ws, "audio_level").await["channel"], "mic");

    status_tx.send(SystemStatus::Failed).unwrap();
    assert_eq!(
        next_of_type(&mut ws, "system_audio_status").await["state"],
        "failed"
    );
    assert_eq!(next_of_type(&mut ws, "audio_level").await["channel"], "mic");
    assert!(
        !pump.is_finished(),
        "the meeting must not stop on a system stall"
    );

    mic.abort();
    relay.abort();
    pump.abort();
    server.abort();
}
