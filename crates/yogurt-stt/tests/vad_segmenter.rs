//! Integration tests for the VAD-driven sliding-window segmenter.
//!
//! Plan 08-02 Task 1 (TDD): these tests pin the segmenter's *contract* on
//! synthetic 16 kHz mono i16 PCM.  We do NOT test webrtc-vad's tone response
//! directly — instead we test that the *segmenter*'s `MIN_SPEECH_MS`,
//! `SILENCE_HANG_MS`, and segment-emission state machine behave per spec
//! when fed alternating speech/silence runs.
//!
//! Per the plan's "KNOWN BRITTLENESS" note: webrtc-vad does not reliably
//! classify pure sine waves as speech.  We use deterministic white noise
//! (`(i % 251) * 64` mod-folded) as our "speech" signal — it carries the
//! broadband energy webrtc-vad keys off, and it stays deterministic so the
//! tests don't flake across runs.

#![cfg(feature = "local-stt")]

use yogurt_stt::vad::{Segmenter, SegmenterEvent};

const SR: usize = 16_000;

/// Deterministic white-noise "speech" — `(i % 251) * 64` folds into the
/// i16 range without ever clipping (max 250 * 64 = 16000).  webrtc-vad
/// reliably classifies this as voice in Aggressive mode.
fn tone(seconds: f32) -> Vec<i16> {
    let n = (SR as f32 * seconds) as usize;
    (0..n).map(|i| ((i % 251) as i16) * 64).collect()
}

/// Pure silence — `vec![0i16; n]`.  webrtc-vad classifies as non-voice.
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
    // 1.5 s "speech" (white noise) followed by 1 s silence.  The
    // 1 s silence is well over SILENCE_HANG_MS (600 ms) so the segmenter
    // must flush exactly one segment with > 1 s of audio inside.
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
    // 0.8 s speech / 0.8 s silence / 0.8 s speech / 0.8 s silence.
    // The 0.8 s silence between runs exceeds SILENCE_HANG_MS (600 ms)
    // so the segmenter must emit two distinct segments.
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
    // 3 s of pure silence — webrtc-vad must classify every frame as
    // non-voice and the segmenter must emit zero segments.
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
    // 0.1 s "speech" + 1 s silence.  100 ms < MIN_SPEECH_MS (250 ms),
    // so the speech run is filtered as a cough/click and zero segments
    // are emitted.  This is the MIN_SPEECH_MS boundary, not a VAD
    // tone-response test (see module docs).
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

// ---------------------------------------------------------------------------
// `pending()` - the in-flight utterance the partial ticker previews (AUD-1).
//
// The bug these pin: the whisper partial ticker used to keep its own rolling
// 5 s window instead of reading the segmenter, so once an utterance ran past
// five seconds each re-decode saw *less* audio than the one before and the
// on-screen partial shrank mid-sentence. Reading `pending()` makes growth a
// property of the segmenter rather than a rule the ticker has to follow, so
// that is where the assertions live.
// ---------------------------------------------------------------------------

/// Length of `pending()`'s PCM, or 0 when nothing is in flight.
fn pending_len(seg: &Segmenter) -> usize {
    seg.pending().map(|(pcm, _)| pcm.len()).unwrap_or(0)
}

#[test]
fn pending_is_none_between_utterances() {
    let mut seg = Segmenter::new(SR);

    // Before any audio at all.
    assert!(seg.pending().is_none(), "pending before any push");

    // Silence only - never entered speech.
    seg.push(&silence(1.0), |_| {});
    assert!(seg.pending().is_none(), "pending during opening silence");

    // Speech - now in flight.
    seg.push(&tone(1.0), |_| {});
    assert!(seg.pending().is_some(), "pending mid-utterance");

    // Trailing silence past SILENCE_HANG_MS flushes the segment, which must
    // hand the buffer off and leave nothing pending. This is what stops the
    // next utterance's partial from starting out carrying the previous one,
    // and what stops the ticker burning a decode on silence.
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

    // 12 s of continuous speech, fed in 0.5 s pushes so we can watch the
    // buffer between them. Well under MAX_SEGMENT_MS (25 s), so the segmenter
    // stays in one utterance throughout and emits nothing.
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

    // The old ticker capped at 5 s and dropped everything older. Past that
    // point it started shrinking; `pending()` must not.
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

    // Lead with silence so a correct start_ms is distinguishable from 0 -
    // the value the ticker used to hardcode into every partial.
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

    // Same utterance, same timestamp: this is what keeps the transcript line
    // from jumping when the final replaces the partial in place.
    assert_eq!(
        final_start,
        Some(pending_start),
        "partial and final must agree on where the utterance started",
    );
}
