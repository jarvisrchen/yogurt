//! Integration tests pinning the segmenter's contract on synthetic 16 kHz mono i16 PCM.
//!
//! webrtc-vad does not reliably classify pure sine waves as speech, so deterministic white noise stands in for speech.
//! The tests target `MIN_SPEECH_MS`, `SILENCE_HANG_MS` and segment emission, not webrtc-vad's tone response.

#![cfg(feature = "local-stt")]

use yogurt_stt::vad::{Segmenter, SegmenterEvent};

const SR: usize = 16_000;

/// Deterministic white-noise "speech" that never clips (max 250 * 64 = 16000) and that webrtc-vad classifies as voice in Aggressive mode.
fn tone(seconds: f32) -> Vec<i16> {
    let n = (SR as f32 * seconds) as usize;
    (0..n).map(|i| ((i % 251) as i16) * 64).collect()
}

/// Pure silence, which webrtc-vad classifies as non-voice.
fn silence(seconds: f32) -> Vec<i16> {
    let n = (SR as f32 * seconds) as usize;
    vec![0i16; n]
}

fn collect_segments(pcm: &[i16]) -> Vec<SegmenterEvent> {
    let mut seg = Segmenter::new(SR);
    let mut events = Vec::new();
    seg.push(pcm, |e| events.push(e));
    events
}

fn count_segments(events: &[SegmenterEvent]) -> usize {
    events
        .iter()
        .filter(|e| matches!(e, SegmenterEvent::Segment { .. }))
        .count()
}

#[test]
fn it_emits_one_segment_for_speech_then_silence() {
    // 1 s of silence exceeds `SILENCE_HANG_MS`, so exactly one segment with over 1 s of audio flushes.
    let mut pcm = tone(1.5);
    pcm.extend(silence(1.0));

    let events = collect_segments(&pcm);
    assert_eq!(
        count_segments(&events),
        1,
        "expected exactly 1 Segment; got {:#?}",
        events
    );

    let segment = events
        .iter()
        .find_map(|e| match e {
            SegmenterEvent::Segment {
                pcm,
                start_ms,
                end_ms,
            } => Some((pcm.len(), *start_ms, *end_ms)),
            _ => None,
        })
        .unwrap();
    let (pcm_len, start_ms, end_ms) = segment;
    assert!(
        pcm_len > 16_000,
        "segment PCM should be > 1 s of audio at 16 kHz; got {} samples",
        pcm_len
    );
    assert!(
        end_ms.saturating_sub(start_ms) >= 1_000,
        "segment duration should be ≥ 1000 ms; got start={} end={}",
        start_ms,
        end_ms
    );
}

#[test]
fn it_splits_two_speech_runs_separated_by_silence() {
    // The silence between runs exceeds `SILENCE_HANG_MS`, so two segments are emitted.
    let mut pcm = tone(0.8);
    pcm.extend(silence(0.8));
    pcm.extend(tone(0.8));
    pcm.extend(silence(0.8));

    let events = collect_segments(&pcm);
    assert_eq!(
        count_segments(&events),
        2,
        "expected exactly 2 Segments; got {:#?}",
        events
    );
}

#[test]
fn pure_silence_emits_no_segments() {
    // Every frame is non-voice, so zero segments.
    let pcm = silence(3.0);
    let events = collect_segments(&pcm);
    assert_eq!(
        count_segments(&events),
        0,
        "pure silence must not emit segments; got {:#?}",
        events
    );
}

#[test]
fn very_short_speech_blips_are_ignored() {
    // 100 ms is under `MIN_SPEECH_MS`, so the run is filtered as a cough or click.
    let mut pcm = tone(0.1);
    pcm.extend(silence(1.0));

    let events = collect_segments(&pcm);
    assert_eq!(
        count_segments(&events),
        0,
        "short blips below MIN_SPEECH_MS must be filtered; got {:#?}",
        events
    );
}

/// Length of `pending()`'s PCM, or 0 when nothing is in flight.
fn pending_len(seg: &Segmenter) -> usize {
    seg.pending().map(|(pcm, _)| pcm.len()).unwrap_or(0)
}

#[test]
fn pending_is_none_between_utterances() {
    let mut seg = Segmenter::new(SR);

    assert!(seg.pending().is_none(), "pending before any push");

    seg.push(&silence(1.0), |_| {});
    assert!(seg.pending().is_none(), "pending during opening silence");

    seg.push(&tone(1.0), |_| {});
    assert!(seg.pending().is_some(), "pending mid-utterance");

    // A flush hands off the buffer and leaves nothing pending, so the next partial does not carry the previous utterance and the ticker skips silence.
    let mut segments = 0;
    seg.push(&silence(1.0), |e| {
        if matches!(e, SegmenterEvent::Segment { .. }) {
            segments += 1;
        }
    });
    assert_eq!(segments, 1, "expected the utterance to flush");
    assert!(
        seg.pending().is_none(),
        "pending after the segment was emitted"
    );
}

#[test]
fn pending_grows_monotonically_and_past_the_old_five_second_window() {
    let mut seg = Segmenter::new(SR);

    // Fed in 0.5 s pushes to watch the buffer. 12 s is under `MAX_SEGMENT_MS`, so it stays one utterance.
    let mut last = 0usize;
    let mut samples = Vec::new();
    for _ in 0..24 {
        seg.push(&tone(0.5), |e| samples.push(e));
        let now = pending_len(&seg);
        assert!(
            now >= last,
            "pending shrank mid-utterance: {last} -> {now} samples (AUD-1)",
        );
        last = now;
    }
    assert!(
        samples
            .iter()
            .all(|e| !matches!(e, SegmenterEvent::Segment { .. })),
        "12 s of unbroken speech should not flush a segment",
    );

    // Growth must continue past 5 s, where a rolling window would start shrinking.
    assert!(
        last > SR * 5,
        "pending capped at {last} samples, expected > {} (5 s)",
        SR * 5,
    );
    assert!(
        last >= SR * 11,
        "pending held only {last} samples after 12 s of speech",
    );
}

#[test]
fn pending_start_ms_matches_the_final_segment() {
    let mut seg = Segmenter::new(SR);

    // Lead with silence so a correct `start_ms` is distinguishable from 0.
    seg.push(&silence(1.0), |_| {});
    seg.push(&tone(1.5), |_| {});

    let (_, pending_start) = seg.pending().expect("utterance in flight");
    assert!(
        pending_start > 0,
        "start_ms should be the utterance's own start, not 0",
    );

    let mut final_start = None;
    seg.push(&silence(1.0), |e| {
        if let SegmenterEvent::Segment { start_ms, .. } = e {
            final_start = Some(start_ms);
        }
    });

    // Same utterance, same timestamp, so the line does not jump when the final replaces the partial.
    assert_eq!(
        final_start,
        Some(pending_start),
        "partial and final must agree on where the utterance started",
    );
}
#[test]
fn buffer_is_bounded_in_wall_clock_not_just_voice_time() {
    let mut seg = Segmenter::new(SR);
    let mut max_pending = 0usize;
    // 300 ms voice and 500 ms silence repeated: every voice frame resets `silence_ms`, and `speech_ms` advances at 37.5% of wall-clock,
    // so `MAX_SEGMENT_MS` alone takes 66 s of audio to trip.
    for _ in 0..100 {
        seg.push(&tone(0.3), |_| {});
        seg.push(&silence(0.5), |_| {});
        max_pending = max_pending.max(seg.pending().map(|(p, _)| p.len()).unwrap_or(0));
    }
    assert!(
        max_pending <= SR * 26,
        "pending grew to {:.1} s of audio; whisper re-encodes past 30 s, \
         so the 1 s partial tick budget is blown",
        max_pending as f32 / SR as f32,
    );
}
