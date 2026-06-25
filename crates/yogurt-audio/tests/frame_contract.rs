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
    assert_eq!(f.monotonic_ms, 0);
}

#[test]
#[should_panic(expected = "FRAME_SAMPLES")]
fn it_panics_on_wrong_length() {
    let _ = Frame::new(Channel::Mic, 0, vec![0i16; 100]);
}
