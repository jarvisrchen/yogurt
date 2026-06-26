//! Pluggable speech-to-text for yogurt.
//!
//! The [`Stt`] trait is the extension point. Phase 3 ships [`deepgram::DeepgramStt`];
//! Phase 8 will add a `whisper.cpp` adapter behind the same trait.
//!
//! ## Data flow
//!
//! ```text
//!   yogurt-audio  ──(broadcast<AudioChunk>)──►  yogurt-stt impl  ──(broadcast<TranscriptEvent>)──►  yogurt-server WS
//! ```
//!
//! The `Stt` impl is responsible for:
//!   1. Subscribing to the audio receiver.
//!   2. Forwarding samples to the underlying engine (cloud WS or local model).
//!   3. Translating engine output into [`TranscriptEvent`]s on the transcript sender.
//!   4. Cleaning up its session when the audio stream ends or its task is dropped.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub mod deepgram;

// Phase 8 (Plan 08-01): streaming SHA256 helper used by the model download
// flow (Plan 08-02). Gated behind `local-stt` so the default build does NOT
// pull in `sha2` / `hex` — keeping the no-default-features check fast.
#[cfg(feature = "local-stt")]
pub mod sha256;

/// Which channel a transcript line came from.
///
/// Granola itself only does "Me"/"Them" (PRD §5.2 explicitly limits diarization to this).
/// `Mic` renders as "Me" (ink black). `System` renders as "Them" (grey).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    Mic,
    System,
}

/// One frame of audio from `yogurt-audio`. Mirrors the type that crate broadcasts
/// (we don't depend on it directly to avoid a circular dep — the server is the wirer).
///
/// 16 kHz mono i16 PCM. Length is implementation-defined (Deepgram tolerates any
/// chunk size; `yogurt-audio` produces ~20ms chunks = 320 samples).
#[derive(Debug, Clone)]
pub struct AudioChunk {
    pub channel: Channel,
    pub samples: Vec<i16>,
    /// Milliseconds since meeting start (set by the producer, not the STT impl).
    pub ts_ms: u64,
}

/// A transcript event produced by the STT engine.
///
/// Matches the `S→C transcript` payload from PRD §10 verbatim (modulo case via serde
/// rename_all) so we can `serde_json::to_string` it straight onto the wire.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TranscriptEvent {
    pub ts_ms: u64,
    pub channel: Channel,
    pub text: String,
    /// `true` when the engine considers this segment locked in. Deepgram sends
    /// `is_final: true` after its end-of-utterance heuristic fires.
    pub is_final: bool,
}

pub type AudioRx = tokio::sync::broadcast::Receiver<AudioChunk>;
pub type TranscriptTx = tokio::sync::broadcast::Sender<TranscriptEvent>;

/// The Stt trait. Implementations run their session for the lifetime of the
/// returned future — call `tokio::spawn` to run in the background, then drop the
/// returned `JoinHandle` to cancel.
///
/// Returns when the audio stream closes (broadcast channel is empty + all senders
/// dropped) OR when the engine signals end-of-stream. Errors propagate to the caller.
#[async_trait]
pub trait Stt: Send + Sync {
    async fn start(&self, audio_rx: AudioRx, txn: TranscriptTx) -> anyhow::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn it_serializes_transcript_event_to_prd_shape() {
        let ev = TranscriptEvent {
            ts_ms: 11_020,
            channel: Channel::Mic,
            text: "hello world".into(),
            is_final: true,
        };
        let json = serde_json::to_value(&ev).unwrap();
        // PRD §10: `{ts_ms, channel, text, is_final}`. `channel` is "mic" lowercase
        // (rename_all on the enum). Snake-case on field names.
        assert_eq!(json["ts_ms"], 11_020);
        assert_eq!(json["channel"], "mic");
        assert_eq!(json["text"], "hello world");
        assert_eq!(json["is_final"], true);
    }
}
