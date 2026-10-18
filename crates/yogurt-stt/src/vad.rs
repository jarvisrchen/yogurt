//! Turns 16 kHz mono i16 PCM into utterance-bounded `Segment` events using `webrtc-vad` in Aggressive mode, tuned for noisy meeting audio.
//!
//! Time is tracked in `cursor_ms`, advanced per 30 ms frame, so the segmenter is deterministic on its input.
//! Segments keep the trailing silence so Whisper catches final phonemes, and runs under `MIN_SPEECH_MS` are discarded as coughs or clicks.
//! 30 ms is the longest frame webrtc-vad accepts, which gives it the most context per decision.
use webrtc_vad::{SampleRate, Vad, VadMode};

/// Frame length in milliseconds.
pub const FRAME_MS: u64 = 30;

/// Sample rate (Hz), matching `yogurt-audio`'s resampled output.
pub const SAMPLE_RATE_HZ: usize = 16_000;

/// Samples per frame.
pub const FRAME_SAMPLES: usize = 480;

/// Minimum speech run that counts as an utterance, filtering coughs, clicks and single syllables.
pub const MIN_SPEECH_MS: u64 = 250;

/// Silence tail required before an utterance is flushed. Mirrors Granola's ~600 ms hangover.
pub const SILENCE_HANG_MS: u64 = 600;

/// Hard cap on a single segment so Whisper sees bounded input.
pub const MAX_SEGMENT_MS: u64 = 25_000;

/// The same cap in samples, applied to the buffer itself.
///
/// `speech_ms` counts only voice frames while `buffer` collects every frame during speech,
/// so speech with short pauses reaches 46 s of buffer (at a 37% duty cycle) before `speech_ms` hits 25 s.
/// whisper.cpp encodes in 30 s windows, and anything longer costs a second encoder pass that breaks the partial-decode budget.
pub const MAX_SEGMENT_SAMPLES: usize = SAMPLE_RATE_HZ * MAX_SEGMENT_MS as usize / 1000;

/// Segmenter event surfaced to the WhisperLocal pump.
#[derive(Debug, Clone)]
pub enum SegmenterEvent {
    /// Speech started at `at_ms` (cursor time).
    SpeechStart { at_ms: u64 },
    /// A complete utterance, ready for decode.
    Segment {
        pcm: Vec<i16>,
        start_ms: u64,
        end_ms: u64,
    },
}

/// VAD-driven segmenter.
///
/// `webrtc_vad::Vad` wraps a raw `*mut Fvad` and is `!Send`.
/// The handle is exclusively owned and only touched through `&mut self`, and `WhisperLocal::start`'s future needs `Send`.
pub struct Segmenter {
    vad: Vad,
    /// Partial-frame tail of the previous `push`, always under `FRAME_SAMPLES`.
    leftover: Vec<i16>,
    /// Current utterance PCM.
    buffer: Vec<i16>,
    in_speech: bool,
    speech_start_ms: u64,
    speech_ms: u64,
    silence_ms: u64,
    /// Total ms processed across all `push` calls.
    cursor_ms: u64,
}

// SAFETY: the `*mut Fvad` is exclusively owned by this `Segmenter` and only accessed through `&mut self`.
unsafe impl Send for Segmenter {}

impl Segmenter {
    /// Construct a segmenter for `sample_rate` (Hz), rounded down to a rate webrtc-vad supports (8, 16, 32 or 48 kHz).
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

    /// PCM of the utterance in flight and the `cursor_ms` it started at, or `None` between utterances.
    ///
    /// This is the same buffer the eventual `Segment` carries, so the partial-preview decoder in `whisper_local` only ever sees it grow.
    /// A rolling window would make the displayed partial shrink once an utterance outran it.
    /// The `None` between utterances stops the ticker decoding a silent channel.
    pub fn pending(&self) -> Option<(&[i16], u64)> {
        if !self.in_speech || self.buffer.is_empty() {
            return None;
        }
        Some((&self.buffer, self.speech_start_ms))
    }

    /// Feed PCM samples, sliced into 480-sample frames. A trailing partial frame is kept for the next call.
    pub fn push<F>(&mut self, pcm: &[i16], mut on_event: F)
    where
        F: FnMut(SegmenterEvent),
    {
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
        // Err means a wrong frame length, which cannot happen here, so it counts as no voice.
        let is_voice = self.vad.is_voice_segment(frame).unwrap_or(false);

        if is_voice && !self.in_speech {
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
            // Counts wall-clock, not voice time. See MAX_SEGMENT_SAMPLES.
            let buffer_too_long = self.buffer.len() >= MAX_SEGMENT_SAMPLES;

            if silence_long_enough || speech_too_long || buffer_too_long {
                if self.speech_ms >= MIN_SPEECH_MS {
                    let pcm = std::mem::take(&mut self.buffer);
                    let start_ms = self.speech_start_ms;
                    let end_ms = self.cursor_ms + FRAME_MS;
                    on_event(SegmenterEvent::Segment {
                        pcm,
                        start_ms,
                        end_ms,
                    });
                } else {
                    self.buffer.clear();
                }
                self.in_speech = false;
                self.speech_ms = 0;
                self.silence_ms = 0;
            }
        } else {
            // Defensive bound on `buffer`, which is only appended to while `in_speech`.
            if self.buffer.len() > SAMPLE_RATE_HZ * 2 {
                self.buffer.clear();
            }
        }

        self.cursor_ms += FRAME_MS;
    }
}
