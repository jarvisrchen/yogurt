//! Voice-activity-detection segmenter — Plan 08-02 Task 1.
//!
//! Turns a stream of 16 kHz mono i16 PCM samples into utterance-bounded
//! `Segment` events using `webrtc-vad` in Aggressive mode.  Designed for
//! noisy meeting audio (the project's hero use case), not clean-room
//! lavalier mics.
//!
//! ## State machine
//!
//! The segmenter holds a small ring of partial-frame leftover, the live
//! buffer for the current utterance, and three counters: `speech_ms`,
//! `silence_ms`, `cursor_ms`.  For every 30 ms / 480-sample frame:
//!
//! 1. Run `webrtc-vad::is_voice_segment(frame)`.
//! 2. If voice **and** we were not in speech, transition: emit
//!    `SegmenterEvent::SpeechStart`, set `in_speech = true`,
//!    `speech_start_ms = cursor_ms`, reset `speech_ms / silence_ms`.
//! 3. If voice while in speech, advance `speech_ms`.  Append the frame
//!    to `buffer`.  Reset `silence_ms`.
//! 4. If silence while in speech, advance `silence_ms`.  Still append
//!    the frame so the segment carries the natural tail (helps Whisper
//!    catch trailing phonemes).
//! 5. When `in_speech` AND (`silence_ms ≥ SILENCE_HANG_MS`
//!    OR `speech_ms ≥ MAX_SEGMENT_MS`) AND `speech_ms ≥ MIN_SPEECH_MS`,
//!    emit `SegmenterEvent::Segment { pcm, start_ms, end_ms }` and
//!    reset to non-speech state.
//! 6. When we hit the SILENCE_HANG_MS gate but `speech_ms <
//!    MIN_SPEECH_MS`, discard the buffer (it was a cough / click).
//!
//! All time is tracked in `cursor_ms`, advanced by FRAME_MS per frame
//! we process — there is no wall-clock dependency, the segmenter is
//! purely deterministic on its input.
//!
//! ## Why 30 ms frames
//!
//! webrtc-vad accepts 10 / 20 / 30 ms frames at 16 kHz (160/320/480 samples).
//! 30 ms is the longest, which gives the VAD the most context per decision
//! and minimizes per-frame overhead.  Plan 08-02 source-plan Step 3 pins
//! this explicitly.

use webrtc_vad::{SampleRate, Vad, VadMode};

/// Frame length in milliseconds.  webrtc-vad supports 10/20/30 ms;
/// 30 ms gives the most context per decision.
pub const FRAME_MS: u64 = 30;

/// Sample rate (Hz).  Matches `yogurt-audio` post-resample output.
pub const SAMPLE_RATE_HZ: usize = 16_000;

/// Samples per 30 ms frame at 16 kHz.  `SAMPLE_RATE_HZ * FRAME_MS / 1000`.
pub const FRAME_SAMPLES: usize = 480;

/// Minimum speech run that counts as an utterance.  Runs shorter than
/// this are filtered — coughs, clicks, single-word "uh" syllables.
pub const MIN_SPEECH_MS: u64 = 250;

/// How long the silence tail must run before we flush an utterance.
/// Empirically tuned; mirrors Granola's ~600 ms hangover.
pub const SILENCE_HANG_MS: u64 = 600;

/// Hard cap on a single segment.  If someone monologues for > 25 s
/// without a pause, we cut anyway so Whisper sees bounded input.
pub const MAX_SEGMENT_MS: u64 = 25_000;

/// Segmenter event surfaced to the WhisperLocal pump.
#[derive(Debug, Clone)]
pub enum SegmenterEvent {
    /// Speech started at `at_ms` (cursor time).  Useful for UI
    /// indicators ("listening...") and not strictly required by the
    /// decoder path.
    SpeechStart { at_ms: u64 },
    /// A complete utterance, ready for decode.  Owned PCM so the
    /// receiver can move it into `spawn_blocking` without lifetime
    /// hassle.
    Segment {
        pcm: Vec<i16>,
        start_ms: u64,
        end_ms: u64,
    },
}

/// VAD-driven sliding-window segmenter.
///
/// `webrtc_vad::Vad` wraps a raw `*mut Fvad` C-handle and is therefore
/// `!Send` by default.  We own the handle exclusively (no aliasing,
/// no shared state) and only ever access it through `&mut self`, so
/// it is safe to move the `Segmenter` across threads.  The
/// `unsafe impl Send` below makes that explicit; the async-trait
/// future inside `WhisperLocal::start` requires `Send` to satisfy
/// `Stt: Send + Sync`.
pub struct Segmenter {
    vad: Vad,
    /// Partial-frame leftover from the previous `push` call.  When
    /// the caller hands us a chunk that doesn't divide evenly into
    /// 480-sample frames, we stash the tail here and prepend it next
    /// time.  Bounded at `< FRAME_SAMPLES`.
    leftover: Vec<i16>,
    /// Current utterance PCM (owned).  Cleared after each `Segment`
    /// emit, or trimmed if a long silence with no speech accumulates.
    buffer: Vec<i16>,
    in_speech: bool,
    speech_start_ms: u64,
    /// ms of contiguous voice frames inside the current run.
    speech_ms: u64,
    /// ms of contiguous silence frames inside the current run
    /// (resets to 0 on each voice frame while `in_speech`).
    silence_ms: u64,
    /// Total ms processed across all `push` calls — wall-clock-ish
    /// cursor used to stamp segment boundaries.
    cursor_ms: u64,
}

// SAFETY: the underlying `*mut Fvad` is exclusively owned by this
// `Segmenter` — never aliased, never shared — and is only accessed
// through `&mut self` methods.  Moving the `Segmenter` between
// threads is therefore sound.
unsafe impl Send for Segmenter {}

impl Segmenter {
    /// Construct a segmenter for `sample_rate` (Hz).  webrtc-vad
    /// requires one of 8000 / 16000 / 32000 / 48000.  Anything else
    /// is rounded down to the nearest supported rate.
    pub fn new(sample_rate: usize) -> Self {
        let rate = match sample_rate {
            r if r >= 48_000 => SampleRate::Rate48kHz,
            r if r >= 32_000 => SampleRate::Rate32kHz,
            r if r >= 16_000 => SampleRate::Rate16kHz,
            _ => SampleRate::Rate8kHz,
        };
        let vad = Vad::new_with_rate_and_mode(rate, VadMode::Aggressive);
        Self {
            vad,
            leftover: Vec::with_capacity(FRAME_SAMPLES),
            buffer: Vec::new(),
            in_speech: false,
            speech_start_ms: 0,
            speech_ms: 0,
            silence_ms: 0,
            cursor_ms: 0,
        }
    }

    /// Feed PCM samples.  Frames are sliced internally to 480 samples
    /// (30 ms @ 16 kHz); any trailing partial frame is retained for
    /// the next call.  The closure is invoked once per emitted event.
    pub fn push<F>(&mut self, pcm: &[i16], mut on_event: F)
    where
        F: FnMut(SegmenterEvent),
    {
        // Concatenate leftover + new samples, then slice into 480-sample
        // frames.  Avoiding the explicit `Vec::extend` here would save
        // an allocation on the hot path, but for now correctness > μs.
        let mut combined = Vec::with_capacity(self.leftover.len() + pcm.len());
        combined.extend_from_slice(&self.leftover);
        combined.extend_from_slice(pcm);
        self.leftover.clear();

        let full_frames = combined.len() / FRAME_SAMPLES;
        for i in 0..full_frames {
            let start = i * FRAME_SAMPLES;
            let end = start + FRAME_SAMPLES;
            let frame = &combined[start..end];
            self.process_frame(frame, &mut on_event);
        }

        // Stash the partial tail.
        let tail_start = full_frames * FRAME_SAMPLES;
        if tail_start < combined.len() {
            self.leftover.extend_from_slice(&combined[tail_start..]);
        }
    }

    fn process_frame<F>(&mut self, frame: &[i16], on_event: &mut F)
    where
        F: FnMut(SegmenterEvent),
    {
        debug_assert_eq!(frame.len(), FRAME_SAMPLES);
        // webrtc-vad returns Err(()) only if the frame length is wrong
        // for the configured sample rate — by construction we always
        // pass 480 samples at 16 kHz, so we treat Err as "no voice".
        let is_voice = self.vad.is_voice_segment(frame).unwrap_or(false);

        if is_voice && !self.in_speech {
            // Transition into speech.
            self.in_speech = true;
            self.speech_start_ms = self.cursor_ms;
            self.speech_ms = 0;
            self.silence_ms = 0;
            self.buffer.clear();
            on_event(SegmenterEvent::SpeechStart {
                at_ms: self.cursor_ms,
            });
        }

        if self.in_speech {
            self.buffer.extend_from_slice(frame);
            if is_voice {
                self.speech_ms += FRAME_MS;
                self.silence_ms = 0;
            } else {
                self.silence_ms += FRAME_MS;
            }

            let silence_long_enough = self.silence_ms >= SILENCE_HANG_MS;
            let speech_too_long = self.speech_ms >= MAX_SEGMENT_MS;

            if silence_long_enough || speech_too_long {
                if self.speech_ms >= MIN_SPEECH_MS {
                    // Flush a real segment.
                    let pcm = std::mem::take(&mut self.buffer);
                    let start_ms = self.speech_start_ms;
                    // cursor_ms is advanced after this block, so the
                    // current frame's end is cursor_ms + FRAME_MS.
                    let end_ms = self.cursor_ms + FRAME_MS;
                    on_event(SegmenterEvent::Segment {
                        pcm,
                        start_ms,
                        end_ms,
                    });
                } else {
                    // Discard — was just a cough / click.
                    self.buffer.clear();
                }
                self.in_speech = false;
                self.speech_ms = 0;
                self.silence_ms = 0;
            }
        } else {
            // Bound buffer growth during long silence runs — we'd
            // never actually grow `buffer` here (we only append while
            // in_speech), but if a caller transitions us into speech
            // and then we sit in silence-purgatory between transitions
            // the leftover should still be bounded.  Belt + suspenders.
            if self.buffer.len() > SAMPLE_RATE_HZ * 2 {
                self.buffer.clear();
            }
        }

        self.cursor_ms += FRAME_MS;
    }
}
