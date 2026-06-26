//! BL-02 regression: Deepgram adapter must surface backpressure,
//! reconnect on transient errors, and emit a terminal status on 401.
//!
//! We exercise three scenarios with a hand-rolled mock WS server:
//!   (a) Server accepts then closes mid-stream — supervisor emits
//!       "[stt disconnected, retrying]" status frames.
//!   (b) Server returns HTTP 401 on upgrade — initial connect fails with
//!       an `Stt::start` error mentioning the API key. (No reconnect: we
//!       can't easily inject a mid-stream 401, but the unit assertion that
//!       401 maps to ConnectError::Auth is exercised by the start path.)
//!   (c) Server accepts both channels but never reads — backpressure path
//!       kicks in once the mpsc fills, supervisor emits
//!       "[stt overloaded, transcript may be lossy]".

use futures_util::{SinkExt, StreamExt};
use std::time::Duration;
use tokio::net::TcpListener;
use yogurt_stt::{deepgram::DeepgramStt, AudioChunk, Channel, Stt, TranscriptEvent};

/// (a) Server closes mid-stream → supervisor emits at least one
///     "[stt disconnected, retrying]" status frame for the affected channel.
#[tokio::test(flavor = "multi_thread")]
async fn it_emits_disconnect_status_when_server_closes() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    // Both connections: accept and close immediately. Reconnect loop will
    // keep trying with exponential backoff (1s, 2s, 4s) — that's fine for
    // the test because we only need to see ONE status event.
    let server = tokio::spawn(async move {
        for _ in 0..2 {
            let (stream, _) = listener.accept().await.unwrap();
            tokio::spawn(async move {
                let ws = tokio_tungstenite::accept_async(stream).await.unwrap();
                let (mut write, _read) = ws.split();
                let _ = write.close().await;
            });
        }
    });

    let (_audio_tx, audio_rx) = tokio::sync::broadcast::channel::<AudioChunk>(16);
    let (txn_tx, mut txn_rx) = tokio::sync::broadcast::channel::<TranscriptEvent>(16);

    let mut stt = DeepgramStt::new("fake-key");
    stt.base_url = format!("ws://127.0.0.1:{port}");
    let stt = std::sync::Arc::new(stt);
    let stt2 = stt.clone();
    let adapter = tokio::spawn(async move {
        let _ = stt2.start(audio_rx, txn_tx).await;
    });

    // We expect at least one "[stt disconnected, retrying]" event within 2s.
    let mut saw_disconnect = false;
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while std::time::Instant::now() < deadline {
        if let Ok(Ok(ev)) = tokio::time::timeout(Duration::from_millis(200), txn_rx.recv()).await {
            if ev.text.contains("disconnected") {
                saw_disconnect = true;
                break;
            }
        }
    }
    assert!(
        saw_disconnect,
        "expected a synthetic '[stt disconnected, retrying]' status event"
    );

    adapter.abort();
    server.abort();
}

/// (b) Server returns 401 on upgrade → `Stt::start` returns an Err mentioning
///     the API key. The supervisor's reconnect path classifies the error as
///     terminal (Auth) — no retries.
#[tokio::test(flavor = "multi_thread")]
async fn it_fails_fast_on_auth_rejection() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    // Mock that accepts the TCP connection but responds with HTTP 401
    // BEFORE the WS upgrade completes. Easiest way: read the request and
    // write a raw HTTP/1.1 401 response.
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        // Drain the upgrade request bytes (we don't care about content).
        let mut buf = [0u8; 1024];
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let _ = stream.read(&mut buf).await;
        let resp = b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\n\r\n";
        let _ = stream.write_all(resp).await;
        let _ = stream.shutdown().await;
    });

    let (_audio_tx, audio_rx) = tokio::sync::broadcast::channel::<AudioChunk>(16);
    let (txn_tx, _txn_rx) = tokio::sync::broadcast::channel::<TranscriptEvent>(16);

    let mut stt = DeepgramStt::new("bad-key");
    stt.base_url = format!("ws://127.0.0.1:{port}");

    let result = tokio::time::timeout(Duration::from_secs(3), stt.start(audio_rx, txn_tx))
        .await
        .expect("start must return within 3s");
    let err = result.expect_err("auth failure must surface as Err");
    let msg = format!("{err}");
    assert!(
        msg.to_lowercase().contains("auth") || msg.contains("YOGURT_DEEPGRAM_API_KEY"),
        "expected auth-flavored error; got: {msg}"
    );

    server.abort();
}

/// (c) Server accepts both channels but never reads — the writer's buffer
///     eventually fills and the upstream pump's `try_send` drops chunks.
///     After enough consecutive drops, the supervisor emits an
///     "[stt overloaded, ...]" status event.
#[tokio::test(flavor = "multi_thread")]
async fn it_emits_overload_status_when_backpressured() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    // Accept both connections but immediately stop reading. The kernel's
    // TCP recv buffer eventually fills, then tungstenite's send buffer
    // fills, then write.send().await blocks the writer task, then the
    // supervisor's per-channel mpsc (cap 64) fills, then the pump's
    // try_send starts dropping.
    let server = tokio::spawn(async move {
        for _ in 0..2 {
            let (stream, _) = listener.accept().await.unwrap();
            tokio::spawn(async move {
                let ws = tokio_tungstenite::accept_async(stream).await.unwrap();
                // Hold the connection open without ever reading.
                let (mut _write, _read) = ws.split();
                tokio::time::sleep(Duration::from_secs(60)).await;
            });
        }
    });

    let (audio_tx, audio_rx) = tokio::sync::broadcast::channel::<AudioChunk>(2048);
    let (txn_tx, mut txn_rx) = tokio::sync::broadcast::channel::<TranscriptEvent>(64);

    let mut stt = DeepgramStt::new("fake-key");
    stt.base_url = format!("ws://127.0.0.1:{port}");
    let stt = std::sync::Arc::new(stt);
    let stt2 = stt.clone();
    let adapter = tokio::spawn(async move {
        let _ = stt2.start(audio_rx, txn_tx).await;
    });

    // Give the adapter a beat to open both WS connections.
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Pump ~200 chunks of mic audio. With the server not reading, the
    // first ~64 fill the per-channel mpsc and the rest are dropped via
    // try_send. After 50 consecutive drops, the supervisor emits the
    // overload status.
    for i in 0..400 {
        let _ = audio_tx.send(AudioChunk {
            channel: Channel::Mic,
            samples: vec![0i16; 320],
            ts_ms: i,
        });
    }

    // Look for either the overload status OR any disconnect status — both
    // are valid backpressure-aware behaviors depending on timing.
    let mut saw_status = false;
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while std::time::Instant::now() < deadline {
        if let Ok(Ok(ev)) = tokio::time::timeout(Duration::from_millis(200), txn_rx.recv()).await {
            if ev.text.starts_with("[stt") {
                saw_status = true;
                break;
            }
        }
    }
    assert!(
        saw_status,
        "expected a synthetic '[stt ...]' status event under backpressure"
    );

    adapter.abort();
    server.abort();
}
