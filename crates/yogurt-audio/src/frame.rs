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
    /// Milliseconds since `start_capture()` returned. Used by Phase 3 to
    /// align partial transcripts with notes via `↳ HH:MM` deep-links
    /// (PRD §5.3).
    pub monotonic_ms: u64,
    /// Always [`FRAME_SAMPLES`] samples of mono 16 kHz i16 PCM.
    pub samples: Vec<i16>,
}

impl Frame {
    /// Construct a frame. Panics if `samples.len() != FRAME_SAMPLES` — this
    /// is a programmer error, not a runtime condition the user can recover
    /// from. Producers (`mic.rs`, `system.rs`, `synthetic.rs`) own the
    /// chunking; a wrong length means a bug in the chunker.
    pub fn new(channel: Channel, monotonic_ms: u64, samples: Vec<i16>) -> Self {
        assert_eq!(
            samples.len(),
            FRAME_SAMPLES,
            "Frame::new: samples.len()={} but FRAME_SAMPLES={}",
            samples.len(),
            FRAME_SAMPLES
        );
        Self {
            channel,
            monotonic_ms,
            samples,
        }
    }
}
