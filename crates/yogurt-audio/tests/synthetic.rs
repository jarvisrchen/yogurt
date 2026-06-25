//! Broadcast-plumbing integration tests using the synthetic sine-wave
//! generator. These run on every platform and verify the
//! producer/consumer contract Plan 02 will reuse for real mic + system
//! capture.

use std::time::Duration;
use tokio::sync::broadcast;
use yogurt_audio::{
    synthetic::{spawn_sine_wave, SineWaveConfig},
    Channel, Frame, FRAME_SAMPLES,
};

#[tokio::test]
async fn it_emits_correct_length_frames_at_the_expected_cadence() {
    let (tx, mut rx) = broadcast::channel::<Frame>(64);
    let handle = spawn_sine_wave(
        SineWaveConfig {
            channel: Channel::Mic,
            frequency_hz: 440.0,
            amplitude: 16_000,
        },
        tx,
    );

    // Collect 5 frames (~100 ms of audio).
    let mut frames = Vec::with_capacity(5);
    for _ in 0..5 {
        let f = tokio::time::timeout(Duration::from_millis(500), rx.recv())
            .await
            .expect("frame arrived within 500ms")
            .expect("recv ok");
        frames.push(f);
    }
    handle.abort();

    for f in &frames {
        assert_eq!(f.channel, Channel::Mic);
        assert_eq!(f.samples.len(), FRAME_SAMPLES);
    }

    // Monotonic time should increase by roughly 20 ms per frame.
    for w in frames.windows(2) {
        let dt = w[1].monotonic_ms.saturating_sub(w[0].monotonic_ms);
        assert!(
            (15..=40).contains(&dt),
            "expected ~20ms between frames, got {}ms",
            dt
        );
    }

    // Sine wave should produce non-zero, non-constant samples.
    let s = &frames[0].samples;
    assert!(s.iter().any(|&x| x != 0), "sine wave should not be all-zero");
    assert!(
        s.iter().any(|&x| x != s[0]),
        "sine wave should vary across samples"
    );
}

#[tokio::test]
async fn multiple_subscribers_each_receive_the_same_frames() {
    let (tx, mut rx1) = broadcast::channel::<Frame>(64);
    let mut rx2 = tx.subscribe();
    let handle = spawn_sine_wave(SineWaveConfig::default_for(Channel::System), tx);

    let f1 = tokio::time::timeout(Duration::from_millis(500), rx1.recv())
        .await
        .unwrap()
        .unwrap();
    let f2 = tokio::time::timeout(Duration::from_millis(500), rx2.recv())
        .await
        .unwrap()
        .unwrap();
    handle.abort();

    assert_eq!(f1.monotonic_ms, f2.monotonic_ms);
    assert_eq!(f1.samples, f2.samples);
    assert_eq!(f1.channel, Channel::System);
}
