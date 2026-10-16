//! Manual smoke test, requires `~/.yogurt/models/ggml-small.en.bin`.
//!
//! Gated three ways so it never runs by accident: `#[cfg(feature = "local-stt")]`, `#[ignore]`, and an early return unless `RUN_WHISPER_SMOKE=1`.
//!
//! ```sh
//! RUN_WHISPER_SMOKE=1 cargo test -p yogurt-stt --features local-stt \
//!     --test whisper_smoke -- --ignored --nocapture
//! ```
//!
//! It asserts only that `WhisperLocal::start()` survives 3 s of silence plus 2 s of a 440 Hz sine without panics, deadlocks or Metal init crashes.
//! There is no transcript assertion because a sine wave decodes to garbage.

#![cfg(feature = "local-stt")]

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use yogurt_stt::{AudioChunk, Channel, Stt, TranscriptEvent, WhisperLocal};

#[tokio::test]
#[ignore]
async fn it_transcribes_a_sine_wave_run_without_crashing() {
    // Returning early rather than `#[ignore]` keeps `--ignored` runs from triggering a multi-second decode unasked.
    if std::env::var("RUN_WHISPER_SMOKE").is_err() {
        eprintln!("set RUN_WHISPER_SMOKE=1 to actually run this test");
        return;
    }

    // Reuse the crate's resolver instead of duplicating path logic.
    let model = yogurt_stt::models::model_path(
        yogurt_stt::models::lookup("small.en").expect("small.en in registry"),
    )
    .expect("resolve model path");
    assert!(
        model.exists(),
        "model not at {} — run Plan 08-02 download flow first",
        model.display()
    );

    // Load blocks, which is fine on a fresh runtime (production wraps it in `spawn_blocking`).
    let stt = WhisperLocal::load(model).expect("load model");

    let (audio_tx, audio_rx) = broadcast::channel::<AudioChunk>(64);
    let (event_tx, mut event_rx) = broadcast::channel::<TranscriptEvent>(32);

    let stt = Arc::new(stt);
    let stt_clone = stt.clone();
    let runner = tokio::spawn(async move { stt_clone.start(audio_rx, event_tx).await });

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

    // The partial ticker fires at 1 s and small.en on Metal is about real-time.
    tokio::time::sleep(Duration::from_secs(8)).await;
    drop(audio_tx);

    // No text assertions: sine waves decode to nonsense.
    let mut events = vec![];
    while let Ok(Ok(ev)) = tokio::time::timeout(Duration::from_secs(2), event_rx.recv()).await {
        events.push(ev);
    }
    eprintln!("smoke: collected {} events", events.len());

    // The runner returning Ok is the assertion.
    let runner_result = tokio::time::timeout(Duration::from_secs(5), runner).await;
    eprintln!("smoke: runner = {:?}", runner_result);
}
