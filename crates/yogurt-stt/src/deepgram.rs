//! Deepgram streaming adapter. Real implementation lands in Task 2.

use crate::{AudioRx, Stt, TranscriptTx};
use async_trait::async_trait;

pub struct DeepgramStt {
    pub api_key: String,
    /// Override the WS base URL (used by tests). Defaults to `wss://api.deepgram.com`.
    pub base_url: String,
    pub model: String,
}

impl DeepgramStt {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: "wss://api.deepgram.com".into(),
            model: "nova-2".into(),
        }
    }
}

#[async_trait]
impl Stt for DeepgramStt {
    async fn start(&self, _audio_rx: AudioRx, _txn: TranscriptTx) -> anyhow::Result<()> {
        anyhow::bail!("yogurt-stt: deepgram adapter not yet implemented (task 2)");
    }
}
