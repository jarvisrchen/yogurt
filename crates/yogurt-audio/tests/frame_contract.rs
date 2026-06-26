//! Format-contract integration tests. These pin the load-bearing audio
//! constants (`SAMPLE_RATE_HZ`, `FRAME_SAMPLES`) and the `Frame::new`
//! length-check that downstream Phase 3 STT engines rely on.

use yogurt_audio::{Channel, Frame, FRAME_SAMPLES, SAMPLE_RATE_HZ};

#[test]
fn it_exposes_format_constants() {
    assert_eq!(SAMPLE_RATE_HZ, 16_000);
    assert_eq!(FRAME_SAMPLES, 320, "20ms @ 16kHz = 320 samples");
}

#[test]
fn it_constructs_a_frame_with_correct_length() {
    let samples = vec![0i16; FRAME_SAMPLES];
    let f = Frame::new(Channel::Mic, 0, samples);
    assert_eq!(f.channel, Channel::Mic);
    assert_eq!(f.samples.len(), FRAME_SAMPLES);
    // CR-01: `monotonic_micros` field carries microsecond resolution; the
    // truncated-ms helper still returns 0 here.
    assert_eq!(f.monotonic_micros, 0);
    assert_eq!(f.monotonic_ms(), 0);
}

/// CR-01: roundtrip the µs↔ms helper to lock in the truncation contract
/// the PRD §5.3 deep-link consumers depend on. Phase 3 / 8 alignment
/// consumers MUST use `monotonic_micros` directly.
#[test]
fn monotonic_ms_helper_truncates_microseconds() {
    let samples = vec![0i16; FRAME_SAMPLES];
    // 19_999 µs = still 19 ms after truncation. 20_000 µs = exactly 20 ms.
    let f1 = Frame::new(Channel::Mic, 19_999, samples.clone());
    let f2 = Frame::new(Channel::Mic, 20_000, samples);
    assert_eq!(f1.monotonic_ms(), 19);
    assert_eq!(f2.monotonic_ms(), 20);
}

#[test]
#[should_panic(expected = "FRAME_SAMPLES")]
fn it_panics_on_wrong_length() {
    let _ = Frame::new(Channel::Mic, 0, vec![0i16; 100]);
}
