use serde::{Deserialize, Serialize};

/// Matches Deepgram `linear16` and whisper.cpp `pcm_s16le`.
pub const SAMPLE_RATE_HZ: u32 = 16_000;

/// 20 ms @ 16 kHz.
pub const FRAME_SAMPLES: usize = 320;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    Mic,
    System,
}

#[derive(Debug, Clone)]
pub struct Frame {
    pub channel: Channel,
    /// Microseconds since `start_capture()` returned.
    /// Millisecond truncation plus 20 ms frames leaves zero gaps for consumers that place samples by timestamp.
    pub monotonic_micros: u64,
    pub samples: Vec<i16>,
}

impl Frame {
    /// Panics if `samples.len() != FRAME_SAMPLES`.
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

    /// Truncated milliseconds, for `HH:MM` deep links. Use `monotonic_micros` for cross-channel alignment.
    pub fn monotonic_ms(&self) -> u64 {
        self.monotonic_micros / 1_000
    }
}
