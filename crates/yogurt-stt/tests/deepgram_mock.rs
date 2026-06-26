//! End-to-end mapping test for the Deepgram adapter using a mock WS server.
//!
//! The mock accepts a connection, drains audio frames, then replies with a
//! hand-rolled "Results" JSON. We assert the adapter publishes the matching
//! TranscriptEvent on the broadcast.

use futures_util::{SinkExt, StreamExt};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;
use yogurt_stt::{deepgram::DeepgramStt, AudioChunk, Channel, Stt, TranscriptEvent};

#[tokio::test(flavor = "multi_thread")]
async fn it_pipes_audio_to_mock_and_emits_transcript_event() {
    // 1. Start a mock WS server on an ephemeral port.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    let server = tokio::spawn(async move {
        // Accept TWO connections. DeepgramStt::start opens mic first then system
        // (deterministic order — see deepgram.rs `start` impl), so:
        //   - connection 0 = mic session: drain one audio frame, send a canned
        //     Results frame, hold briefly so the reader sees it, then close.
        //   - connection 1 = system session: accept and immediately close (no
        //     transcript). This keeps the per-channel assertion deterministic:
        //     the only transcript on the broadcast is the mic one we send below.
        for channel_n in 0..2 {
            let (stream, _) = listener.accept().await.unwrap();
            tokio::spawn(async move {
                let ws = tokio_tungstenite::accept_async(stream).await.unwrap();
                let (mut write, mut read) = ws.split();

                if channel_n == 0 {
                    // Mic session: wait for at least one binary frame to prove
                    // audio is flowing.
                    let mut got_audio = false;
                    while let Some(Ok(msg)) = read.next().await {
                        if matches!(msg, Message::Binary(_)) {
                            got_audio = true;
                            break;
                        }
                        if matches!(msg, Message::Text(ref t) if t.contains("CloseStream")) {
                            return;
                        }
                    }
                    assert!(got_audio, "mock: expected to receive audio bytes on mic");

                    // Send a canned Results frame.
                    let frame = r#"{
                      "type": "Results",
                      "channel": {"alternatives": [{"transcript": "the quick brown fox", "confidence": 0.97}]},
                      "is_final": true,
                      "start": 2.5,
                      "duration": 1.1
                    }"#;
                    let _ = write.send(Message::Text(frame.to_string())).await;

                    // Keep the connection open briefly so the reader picks up the frame.
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    let _ = write.close().await;
                } else {
                    // System session: drain quietly then close. We don't assert
                    // on system-side audio (the order of mic-vs-system delivery
                    // is broadcast-recv order, which is non-deterministic).
                    let _ = write.close().await;
                }
            });
        }
    });

    // 2. Wire up broadcast channels.
    let (audio_tx, audio_rx) = tokio::sync::broadcast::channel::<AudioChunk>(16);
    let (txn_tx, mut txn_rx) = tokio::sync::broadcast::channel::<TranscriptEvent>(16);

    // 3. Spawn the adapter, pointed at the mock.
    let mut stt = DeepgramStt::new("fake-key");
    stt.base_url = format!("ws://127.0.0.1:{port}");
    let stt = std::sync::Arc::new(stt);
    let stt2 = stt.clone();
    let adapter = tokio::spawn(async move {
        stt2.start(audio_rx, txn_tx).await.ok();
    });

    // 4. Push some audio.
    tokio::time::sleep(Duration::from_millis(150)).await; // let WS connections complete
    audio_tx
        .send(AudioChunk {
            channel: Channel::Mic,
            samples: vec![0i16; 320],
            ts_ms: 0,
        })
        .unwrap();
    audio_tx
        .send(AudioChunk {
            channel: Channel::System,
            samples: vec![0i16; 320],
            ts_ms: 0,
        })
        .unwrap();

    // 5. Assert we eventually receive the mapped TranscriptEvent. Because
    //    the system mock closes immediately, the supervisor emits one or
    //    more synthetic "[stt ...]" status frames (BL-02). Skip those and
    //    wait for the real transcript line we care about.
    let ev = loop {
        let ev = tokio::time::timeout(Duration::from_secs(3), txn_rx.recv())
            .await
            .expect("transcript event within 3s")
            .expect("event received");
        if ev.text.starts_with("[stt") {
            // Synthetic status frame from the BL-02 supervisor — skip.
            continue;
        }
        break ev;
    };

    assert_eq!(ev.text, "the quick brown fox");
    assert_eq!(ev.channel, Channel::Mic);
    assert!(ev.is_final);
    assert_eq!(ev.ts_ms, 2500);

    // 6. Cleanup.
    drop(audio_tx);
    let _ = tokio::time::timeout(Duration::from_secs(2), adapter).await;
    server.abort();
}
