//! Frame, Channel, and format constants — the load-bearing audio contract
//! every Phase 3 STT consumer relies on. **Do not change these values
//! without updating the Deepgram + whisper.cpp adapters.**

use serde::{Deserialize, Serialize};

/// Sample rate of every [`Frame`]. 16 kHz matches Deepgram `linear16` and
/// whisper.cpp `pcm_s16le` — Phase 3 STT engines consume this directly.
pub const SAMPLE_RATE_HZ: u32 = 16_000;

/// Number of `i16` samples per [`Frame`]. 20 ms @ 16 kHz = 320 samples.
/// 20 ms is the canonical streaming-STT chunk for low-latency partials.
pub const FRAME_SAMPLES: usize = 320;

/// Audio source. Phase 3 routes [`Channel::Mic`] → "Me" (ink black) and
/// [`Channel::System`] → "Them" (grey) per PRD §5.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    /// Microphone input — works on every platform via `cpal`.
    Mic,
    /// System audio loopback — macOS only via `screencapturekit`.
    System,
}

/// One frame of 16 kHz mono i16 PCM. Length is always [`FRAME_SAMPLES`].
#[derive(Debug, Clone)]
pub struct Frame {
    /// Source of this frame's audio.
    pub channel: Channel,
    /// **Microseconds** since `start_capture()` returned. Used by Phase 3
    /// to align partial transcripts with notes (PRD §5.3) AND by any
    /// future offline-alignment work (e.g. Phase 8 whisper.cpp
    /// re-transcription, transcript-to-audio "play this clip" UI) that
    /// needs sub-millisecond precision for cross-channel sample-exact
    /// alignment.
    ///
    /// **CR-01:** the field was previously `monotonic_ms: u64`. Wall-clock
    /// truncation to whole milliseconds combined with the 20 ms frame
    /// size (320 samples @ 16 kHz) produced the static/crackle bug the
    /// `wav_eartest` diagnosis identified — any consumer that placed
    /// samples at `monotonic_ms * 16` left 16-sample zero-wedges between
    /// consecutive frames whenever wall-clock jitter pushed adjacent
    /// frames into the same millisecond bucket. Microsecond resolution
    /// removes the quantization trap entirely.
    ///
    /// Helper [`Frame::monotonic_ms`] returns the truncated-millisecond
    /// view for the PRD §5.3 `↳ HH:MM` deep-link case which only needs
    /// minute resolution.
    pub monotonic_micros: u64,
    /// Always [`FRAME_SAMPLES`] samples of mono 16 kHz i16 PCM.
    pub samples: Vec<i16>,
}

impl Frame {
    /// Construct a frame. Panics if `samples.len() != FRAME_SAMPLES` — this
    /// is a programmer error, not a runtime condition the user can recover
    /// from. Producers (`mic.rs`, `system.rs`, `synthetic.rs`) own the
    /// chunking; a wrong length means a bug in the chunker.
    pub fn new(channel: Channel, monotonic_micros: u64, samples: Vec<i16>) -> Self {
        assert_eq!(
            samples.len(),
            FRAME_SAMPLES,
            "Frame::new: samples.len()={} but FRAME_SAMPLES={}",
            samples.len(),
            FRAME_SAMPLES
        );
        Self {
            channel,
            monotonic_micros,
            samples,
        }
    }

    /// Truncated-millisecond view of [`Self::monotonic_micros`] — useful
    /// for the PRD §5.3 `↳ HH:MM` deep-link case which only needs minute
    /// resolution. **Do not use this for cross-channel sample alignment**;
    /// see the field-level docstring for why microsecond precision is the
    /// only safe choice there (CR-01).
    pub fn monotonic_ms(&self) -> u64 {
        self.monotonic_micros / 1_000
    }
}
