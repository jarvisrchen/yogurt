//! Deepgram streaming adapter — wss://api.deepgram.com/v1/listen.
//!
//! Wire format reference: <https://developers.deepgram.com/docs/streaming>
//! Audio in:  binary frames, 16 kHz mono linear16 PCM (little-endian i16).
//! Events out: JSON text frames with `type: "Results"` + `channel.alternatives[0].transcript`.
//!
//! Each `Stt::start` call opens ONE WS per [`Channel`] — mic and system are
//! transcribed in parallel sessions, so a "Me" line never gets mis-tagged as "Them".
//! This costs 2× Deepgram seconds but is the only correct way to preserve the
//! channel label without speaker diarization (an explicit v1 non-goal, PRD §2).

use crate::{AudioChunk, AudioRx, Channel, Stt, TranscriptEvent, TranscriptTx};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;

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

    /// Build the connect URL with all query params baked in.
    fn connect_url(&self) -> String {
        format!(
            "{base}/v1/listen?model={model}\
             &encoding=linear16&sample_rate=16000&channels=1\
             &interim_results=true&endpointing=300&smart_format=true",
            base = self.base_url,
            model = self.model,
        )
    }
}

#[async_trait]
impl Stt for DeepgramStt {
    async fn start(&self, mut audio_rx: AudioRx, txn: TranscriptTx) -> anyhow::Result<()> {
        // Open two WS sessions — one per channel — and route audio chunks to the
        // matching session. Each session runs in its own spawned task.
        let mic = spawn_session(self, Channel::Mic, txn.clone()).await?;
        let sys = spawn_session(self, Channel::System, txn.clone()).await?;

        // Pump the unified audio stream into the per-channel senders.
        loop {
            let chunk = match audio_rx.recv().await {
                Ok(c) => c,
                // Lagged or closed: log and exit cleanly.
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(n, "deepgram pump: audio receiver lagged");
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    tracing::info!("deepgram pump: audio stream closed");
                    break;
                }
            };
            let dest = match chunk.channel {
                Channel::Mic => &mic,
                Channel::System => &sys,
            };
            if dest.send(chunk).await.is_err() {
                tracing::warn!("deepgram pump: session task gone, exiting");
                break;
            }
        }

        // Drop the senders so the spawned session tasks know to close their WS.
        drop(mic);
        drop(sys);
        Ok(())
    }
}

/// Open one Deepgram WS for one channel and spawn the reader+writer tasks.
/// Returns an mpsc::Sender into which the caller pushes [`AudioChunk`]s for this channel.
async fn spawn_session(
    cfg: &DeepgramStt,
    channel: Channel,
    txn: TranscriptTx,
) -> anyhow::Result<tokio::sync::mpsc::Sender<AudioChunk>> {
    let url = cfg.connect_url();
    tracing::info!(?channel, %url, "deepgram: connecting");

    let mut req = url.into_client_request()?;
    req.headers_mut().insert(
        "Authorization",
        format!("Token {}", cfg.api_key).parse()?,
    );

    let (ws, _resp) = tokio_tungstenite::connect_async(req).await?;
    let (mut write, mut read) = ws.split();

    let (tx, mut rx) = tokio::sync::mpsc::channel::<AudioChunk>(64);

    // Writer: drain mpsc → binary WS frames. When the mpsc closes, send CloseStream.
    tokio::spawn(async move {
        while let Some(chunk) = rx.recv().await {
            let bytes = i16_slice_to_le_bytes(&chunk.samples);
            if write.send(Message::Binary(bytes)).await.is_err() {
                tracing::warn!(?channel, "deepgram writer: ws send failed");
                return;
            }
        }
        // Channel closed → tell Deepgram we're done so it sends any tail results.
        let close = serde_json::json!({ "type": "CloseStream" }).to_string();
        let _ = write.send(Message::Text(close)).await;
        let _ = write.close().await;
    });

    // Reader: WS text frames → JSON → TranscriptEvent → broadcast.
    tokio::spawn(async move {
        while let Some(msg) = read.next().await {
            let msg = match msg {
                Ok(m) => m,
                Err(e) => {
                    tracing::warn!(?channel, ?e, "deepgram reader: ws error");
                    return;
                }
            };
            let text = match msg {
                Message::Text(t) => t,
                Message::Close(_) => {
                    tracing::info!(?channel, "deepgram reader: ws closed by server");
                    return;
                }
                _ => continue, // ignore binary/ping/pong
            };
            if let Some(ev) = parse_deepgram_event(&text, channel) {
                let _ = txn.send(ev); // ignore lagged subscribers
            }
        }
    });

    Ok(tx)
}

fn i16_slice_to_le_bytes(samples: &[i16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(samples.len() * 2);
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

/// Parse one Deepgram JSON frame into a [`TranscriptEvent`].
/// Returns `None` for non-`Results` frames (Metadata, SpeechStarted, etc.) or
/// when the transcript field is empty.
pub(crate) fn parse_deepgram_event(text: &str, channel: Channel) -> Option<TranscriptEvent> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    if v.get("type")?.as_str()? != "Results" {
        return None;
    }
    let alt = v.get("channel")?.get("alternatives")?.get(0)?;
    let transcript = alt.get("transcript")?.as_str()?.trim();
    if transcript.is_empty() {
        return None;
    }
    let start_s = v.get("start")?.as_f64().unwrap_or(0.0);
    let is_final = v.get("is_final").and_then(|x| x.as_bool()).unwrap_or(false);
    Some(TranscriptEvent {
        ts_ms: (start_s * 1000.0) as u64,
        channel,
        text: transcript.to_string(),
        is_final,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_parses_a_final_results_frame() {
        let frame = r#"{
          "type": "Results",
          "channel": {"alternatives": [{"transcript": "hello world", "confidence": 0.99}]},
          "is_final": true,
          "start": 1.42,
          "duration": 0.7
        }"#;
        let ev = parse_deepgram_event(frame, Channel::Mic).expect("should parse");
        assert_eq!(ev.ts_ms, 1420);
        assert_eq!(ev.text, "hello world");
        assert!(ev.is_final);
        assert_eq!(ev.channel, Channel::Mic);
    }

    #[test]
    fn it_ignores_empty_transcripts() {
        let frame = r#"{
          "type": "Results",
          "channel": {"alternatives": [{"transcript": "", "confidence": 0.0}]},
          "is_final": false,
          "start": 0.0
        }"#;
        assert!(parse_deepgram_event(frame, Channel::Mic).is_none());
    }

    #[test]
    fn it_ignores_metadata_frames() {
        let frame = r#"{"type": "Metadata", "request_id": "abc"}"#;
        assert!(parse_deepgram_event(frame, Channel::System).is_none());
    }

    #[test]
    fn it_converts_i16_to_le_bytes_correctly() {
        let bytes = i16_slice_to_le_bytes(&[0, 1, -1, 256]);
        // 0 = 00 00, 1 = 01 00, -1 = ff ff, 256 = 00 01
        assert_eq!(bytes, vec![0x00, 0x00, 0x01, 0x00, 0xff, 0xff, 0x00, 0x01]);
    }
}
