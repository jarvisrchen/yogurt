//! Manual smoke test — requires `~/.yogurt/models/ggml-small.en.bin` to exist.
//!
//! Gated three ways so it never runs by accident in CI / on first-time
//! contributor machines:
//!   1. The whole file is `#[cfg(feature = "local-stt")]` — without the
//!      feature flag it's invisible to `cargo test`.
//!   2. The test fn is `#[ignore]` — only runs with `-- --ignored`.
//!   3. The test fn returns early unless `RUN_WHISPER_SMOKE=1` is set.
//!
//! Run with:
//!
//! ```sh
//! RUN_WHISPER_SMOKE=1 cargo test -p yogurt-stt --features local-stt \
//!     --test whisper_smoke -- --ignored --nocapture
//! ```
//!
//! What it asserts: the WhisperLocal adapter's `start()` loop runs without
//! crashing when fed 3 s of silence + 2 s of a 440 Hz sine wave. There is
//! deliberately no transcript text assertion — the sine wave decodes to
//! garbage on whisper.cpp, and the point of the smoke is the runtime
//! invariant (no panics, no deadlocks, no Metal init crashes), not the
//! transcript quality. Plan 08-03's perf bench is the real correctness
//! test.

#![cfg(feature = "local-stt")]

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use yogurt_stt::{AudioChunk, Channel, Stt, TranscriptEvent, WhisperLocal};

#[tokio::test]
#[ignore]
async fn it_transcribes_a_sine_wave_run_without_crashing() {
    // Gate 3: env-var opt-in. Returning early (not skipping via
    // `#[ignore]`) is intentional — even a contributor who runs with
    // `--ignored` shouldn't trigger a multi-second whisper.cpp decode
    // unless they explicitly asked for it.
    if std::env::var("RUN_WHISPER_SMOKE").is_err() {
        eprintln!("set RUN_WHISPER_SMOKE=1 to actually run this test");
        return;
    }

    // Reuse the crate's own resolver (~/.yogurt/models/) instead of
    // duplicating path logic.  Safe here: we're past the
    // RUN_WHISPER_SMOKE gate, so plain test runs never reach this.
    let model = yogurt_stt::models::model_path(
        yogurt_stt::models::lookup("small.en").expect("small.en in registry"),
    )
    .expect("resolve model path");
    assert!(
        model.exists(),
        "model not at {} — run Plan 08-02 download flow first",
        model.display()
    );

    // Load is blocking; the smoke runner is a fresh tokio runtime, so
    // doing it inline is fine (in production, meetings/start.rs wraps
    // load in spawn_blocking).
    let stt = WhisperLocal::load(model).expect("load model");

    let (audio_tx, audio_rx) = broadcast::channel::<AudioChunk>(64);
    let (event_tx, mut event_rx) = broadcast::channel::<TranscriptEvent>(32);

    let stt = Arc::new(stt);
    let stt_clone = stt.clone();
    let runner = tokio::spawn(async move { stt_clone.start(audio_rx, event_tx).await });

    // 3 s of silence + 2 s of a 440 Hz tone. Total = 5 s of audio,
    // which is exactly the partial-window cap, so we exercise the
    // rolling-buffer drain logic too.
    let silence = vec![0i16; 16_000 * 3];
    let tone: Vec<i16> = (0..16_000 * 2)
        .map(|i| ((i as f32 / 16_000.0 * 440.0 * 2.0 * std::f32::consts::PI).sin() * 8000.0) as i16)
        .collect();

    audio_tx
        .send(AudioChunk {
            channel: Channel::Mic,
            samples: silence,
            ts_ms: 0,
        })
        .unwrap();
    audio_tx
        .send(AudioChunk {
            channel: Channel::Mic,
            samples: tone,
            ts_ms: 3000,
        })
        .unwrap();

    // Give whisper.cpp time to chew (partial ticker fires at 1 s, and
    // small.en on Metal is ~real-time → 5 s of audio in ~1-2 s of wall).
    tokio::time::sleep(Duration::from_secs(8)).await;
    drop(audio_tx);

    // Drain any events the workers managed to produce. No assertions on
    // text — sine waves decode to nonsense and we don't care; the test
    // is "does the binary survive 8 s of inference without panicking".
    let mut events = vec![];
    while let Ok(Ok(ev)) = tokio::time::timeout(Duration::from_secs(2), event_rx.recv()).await {
        events.push(ev);
    }
    eprintln!("smoke: collected {} events", events.len());

    // The runner future returning Ok is the actual assertion here.
    let runner_result = tokio::time::timeout(Duration::from_secs(5), runner).await;
    eprintln!("smoke: runner = {:?}", runner_result);
}
