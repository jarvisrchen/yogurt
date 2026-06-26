//! Voice-activity-detection segmenter placeholder.
//!
//! Plan 08-02 lands the real `webrtc-vad`-backed sliding-window segmenter.
//! Plan 08-01 ships ONLY the type signatures so `whisper_local.rs` can
//! compile and reference them today; the actual VAD wiring (energy gate +
//! 100 ms hop + 30 ms frames + 800 ms hangover) is Task 8.5 in the source
//! plan.
//!
//! Why this placeholder lives here:
//!   1. `whisper_local::WhisperLocal::start` references `Segmenter` and
//!      `SegmenterEvent` to split incoming PCM into utterance-sized chunks
//!      before handing them to the decoder. Without a placeholder, Plan
//!      08-01 can't compile against the trait.
//!   2. Splitting the placeholder into its own module (rather than stubbing
//!      inside `whisper_local.rs`) gives Plan 08-02 a single file to replace
//!      cleanly — `git diff` will land on this file plus tests, not on the
//!      adapter.
//!   3. Public API surface is intentionally minimal: `new(sample_rate)`,
//!      `push(pcm, on_event)`. Anything richer is Plan 08-02's choice.
//!
//! ## Contract Plan 08-02 must honor
//!
//! - `Segmenter::new(sample_rate_hz: u32) -> Self` — Plan 08-01 hardcodes
//!   16_000 at the call sites.
//! - `Segmenter::push(&mut self, pcm: &[i16], on_event: impl FnMut(SegmenterEvent))` —
//!   synchronous; the closure fires once per detected segment boundary.
//! - `SegmenterEvent::Segment { pcm, start_ms, end_ms }` — the variant
//!   `whisper_local::WhisperLocal::start` pattern-matches against. Owning
//!   `Vec<i16>` (not `&[i16]`) so the decoder can move it across an
//!   `mpsc::Sender` into a `spawn_blocking` closure.
//!
//! The placeholder's `push` is a no-op so Plan 08-01 compiles and tests
//! pass. Plan 08-02 replaces this file in full.

/// Segmenter event surfaced to the WhisperLocal pump.
///
/// Plan 08-02 may extend with additional variants (e.g., `Silence`,
/// `VoiceStarted`) but `Segment` is the only one the decoder consumes.
#[derive(Debug, Clone)]
pub enum SegmenterEvent {
    /// A complete utterance, ready for decode. Owned PCM so the receiver
    /// can move it into `spawn_blocking` without lifetime hassle.
    Segment {
        pcm: Vec<i16>,
        start_ms: u64,
        end_ms: u64,
    },
}

/// Voice-activity-detection segmenter (placeholder — see module docs).
pub struct Segmenter {
    #[allow(dead_code)] // Plan 08-02 reads this to configure the VAD.
    sample_rate_hz: u32,
}

impl Segmenter {
    /// Construct a segmenter for `sample_rate_hz` (Plan 08-01 passes 16_000).
    pub fn new(sample_rate_hz: u32) -> Self {
        Self { sample_rate_hz }
    }

    /// Feed PCM samples into the segmenter. The placeholder is a no-op:
    /// it never invokes `on_event`. Plan 08-02 replaces this with the
    /// real webrtc-vad sliding-window implementation that fires
    /// `SegmenterEvent::Segment` at utterance boundaries.
    pub fn push<F>(&mut self, _pcm: &[i16], _on_event: F)
    where
        F: FnMut(SegmenterEvent),
    {
        // Intentionally no-op. Plan 08-02 task 8.5 implements this.
    }
}
