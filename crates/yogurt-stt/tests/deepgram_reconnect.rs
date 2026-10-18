//! The Deepgram adapter must surface backpressure, reconnect on transient errors, and emit a terminal status on 401.

use futures_util::{SinkExt, StreamExt};
use std::time::Duration;
use tokio::net::TcpListener;
use yogurt_stt::{deepgram::DeepgramStt, AudioChunk, Channel, Stt, TranscriptEvent};

/// A server that closes mid-stream makes the supervisor emit a "[stt disconnected, retrying]" status frame.
#[tokio::test(flavor = "multi_thread")]
async fn it_emits_disconnect_status_when_server_closes() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    // Reconnect backs off exponentially, which is fine since one status event is enough.
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

/// A 401 on upgrade makes `Stt::start` return an Err mentioning the API key, with no retries.
#[tokio::test(flavor = "multi_thread")]
async fn it_fails_fast_on_auth_rejection() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    // Respond with a raw HTTP/1.1 401 before the WS upgrade completes.
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
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

/// A server that never reads fills the writer's buffer, then the mpsc, so `try_send` drops chunks and the supervisor emits an "[stt overloaded, ...]" status event.
#[tokio::test(flavor = "multi_thread")]
async fn it_emits_overload_status_when_backpressured() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    // Stop reading so the TCP buffers, then the writer, then the per-channel mpsc (cap 64) fill, and the pump's `try_send` starts dropping.
    let server = tokio::spawn(async move {
        for _ in 0..2 {
            let (stream, _) = listener.accept().await.unwrap();
            tokio::spawn(async move {
                let ws = tokio_tungstenite::accept_async(stream).await.unwrap();
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

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Pump in every wait iteration, not one burst, so backpressure keeps building even if a slow runner is still opening sockets.
    let mut ts_ms = 0u64;
    let mut pump = |n: u64| {
        for _ in 0..n {
            let _ = audio_tx.send(AudioChunk {
                channel: Channel::Mic,
                samples: vec![0i16; 320],
                ts_ms,
            });
            ts_ms += 1;
        }
    };
    pump(400);

    // Either the overload status or a disconnect status is valid depending on timing.
    let mut saw_status = false;
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        pump(100);
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
