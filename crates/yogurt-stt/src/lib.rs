//! Pluggable speech-to-text. The [`Stt`] trait is the extension point.
//!
//! An impl subscribes to the audio receiver, feeds the engine, emits [`TranscriptEvent`]s,
//! and cleans up when the audio stream ends or its task is dropped.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub mod deepgram;

// `local-stt` pulls in sha2 and whisper.cpp's CMake toolchain, which default builds skip.
#[cfg(feature = "local-stt")]
pub mod sha256;

#[cfg(feature = "local-stt")]
pub mod vad;

#[cfg(feature = "local-stt")]
pub mod models;

#[cfg(feature = "local-stt")]
pub mod whisper_local;
#[cfg(feature = "local-stt")]
pub use whisper_local::WhisperLocal;

/// Which channel a transcript line came from. `Mic` renders as "Me", `System` as "Them".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    Mic,
    System,
}

/// One frame of audio from `yogurt-audio`, mirrored here to avoid a circular dependency.
///
/// 16 kHz mono i16 PCM of any length.
#[derive(Debug, Clone)]
pub struct AudioChunk {
    pub channel: Channel,
    pub samples: Vec<i16>,
    /// Milliseconds since meeting start (set by the producer, not the STT impl).
    pub ts_ms: u64,
}

/// A transcript event produced by the STT engine, serialized as-is onto the WebSocket.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TranscriptEvent {
    pub ts_ms: u64,
    pub channel: Channel,
    pub text: String,
    /// `true` when the engine considers this segment locked in.
    pub is_final: bool,
    /// Per-segment confidence in `0.0..=1.0`, or `None` if the engine has none.
    pub confidence: Option<f32>,
}

pub type AudioRx = tokio::sync::broadcast::Receiver<AudioChunk>;
pub type TranscriptTx = tokio::sync::broadcast::Sender<TranscriptEvent>;

/// Implementations run their session for the lifetime of the returned future.
/// It returns when the audio stream closes or the engine signals end-of-stream.
/// Spawn it to run in the background and drop the `JoinHandle` to cancel.
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
            confidence: Some(0.9),
        };
        let json = serde_json::to_value(&ev).unwrap();
        assert_eq!(json["ts_ms"], 11_020);
        assert_eq!(json["channel"], "mic");
        assert_eq!(json["text"], "hello world");
        assert_eq!(json["is_final"], true);
    }
}
