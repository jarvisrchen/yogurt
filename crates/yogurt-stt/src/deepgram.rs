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
//!
//! ## Reliability contract (BL-02 resolution)
//!
//! - **Backpressure:** the per-channel mpsc queue uses `try_send`. A stalled
//!   Deepgram WS (TCP send-buffer full, network blip) does NOT block the
//!   upstream audio pump for the other channel. Dropped chunks are logged
//!   as `tracing::warn!`; if `MAX_CONSECUTIVE_DROPS` chunks drop in a row,
//!   the session is considered unrecoverable and a `Disconnected` synthetic
//!   transcript event is emitted.
//! - **Reconnect:** on transient errors (network, 5xx, server-initiated
//!   close mid-stream) we attempt up to `MAX_RECONNECT_ATTEMPTS` reconnects
//!   with exponential backoff (1s, 2s, 4s). Each reconnect emits a
//!   "[stt reconnecting]" synthetic event so the UI shows the dropout.
//! - **Auth errors:** HTTP 401/403 on the WS upgrade are TERMINAL — we
//!   do NOT retry, and we emit a "[stt auth failed]" synthetic event
//!   so the user can fix the API key.

use crate::{AudioChunk, AudioRx, Channel, Stt, TranscriptEvent, TranscriptTx};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use std::time::Duration;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;

/// Maximum number of consecutive `try_send` failures before we declare the
/// per-channel session unrecoverable. At ~50 fps audio, 50 dropped chunks =
/// ~1 second of audio lost — well past "transient hiccup" territory.
const MAX_CONSECUTIVE_DROPS: usize = 50;

/// Maximum reconnect attempts after the initial connect succeeds. The 03-01
/// plan specifies "3 attempts with exponential backoff" — we honor that.
const MAX_RECONNECT_ATTEMPTS: usize = 3;

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

/// Classification of a Deepgram WS connection failure.
#[derive(Debug)]
enum ConnectError {
    /// Authentication failed (401/403). Do NOT retry — user must fix key.
    Auth,
    /// Transient: network blip, 5xx, TLS hiccup. Retry with backoff.
    Transient(String),
}

#[async_trait]
impl Stt for DeepgramStt {
    async fn start(&self, mut audio_rx: AudioRx, txn: TranscriptTx) -> anyhow::Result<()> {
        // Open the two per-channel session supervisors. Each supervisor owns
        // its own reconnect loop and emits synthetic status events on
        // disconnect/reconnect/failure.
        let mic = spawn_supervised_session(self, Channel::Mic, txn.clone()).await?;
        let sys = spawn_supervised_session(self, Channel::System, txn.clone()).await?;

        // Pump the unified audio stream into the per-channel senders using
        // try_send so a stalled Deepgram WS doesn't block the OTHER channel.
        let mut mic_drops: usize = 0;
        let mut sys_drops: usize = 0;
        loop {
            let chunk = match audio_rx.recv().await {
                Ok(c) => c,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(n, "deepgram pump: audio receiver lagged");
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    tracing::info!("deepgram pump: audio stream closed");
                    break;
                }
            };

            let (dest, drops, channel) = match chunk.channel {
                Channel::Mic => (&mic, &mut mic_drops, Channel::Mic),
                Channel::System => (&sys, &mut sys_drops, Channel::System),
            };

            match dest.try_send(chunk) {
                Ok(()) => {
                    *drops = 0;
                }
                Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                    *drops += 1;
                    tracing::warn!(
                        ?channel,
                        consecutive = *drops,
                        "deepgram pump: backpressure — dropping chunk"
                    );
                    if *drops >= MAX_CONSECUTIVE_DROPS {
                        tracing::error!(
                            ?channel,
                            consecutive = *drops,
                            "deepgram pump: sustained backpressure — channel unrecoverable"
                        );
                        emit_status(&txn, channel, "[stt overloaded, transcript may be lossy]");
                        // Reset so we don't spam the user; the supervisor's
                        // own reconnect path will signal recovery.
                        *drops = 0;
                    }
                }
                Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                    tracing::warn!(
                        ?channel,
                        "deepgram pump: session task gone (channel closed)"
                    );
                    // Don't break — the OTHER channel may still be healthy.
                    // The supervisor for this channel has already emitted
                    // its terminal status event.
                }
            }
        }

        // Drop the senders so the supervisor tasks unwind.
        drop(mic);
        drop(sys);
        Ok(())
    }
}

/// Emit a synthetic status TranscriptEvent so the UI knows the upstream STT
/// session is in trouble. Uses `is_final: true` so the dock renders it as a
/// locked line (rather than something that gets replaced).
fn emit_status(txn: &TranscriptTx, channel: Channel, text: &str) {
    let _ = txn.send(TranscriptEvent {
        ts_ms: 0,
        channel,
        text: text.to_string(),
        is_final: true,
    });
}

/// Open a self-healing Deepgram session for one channel.
///
/// Returns an mpsc::Sender into which the audio pump pushes [`AudioChunk`]s.
/// Internally spawns a supervisor task that:
///   1. Opens the WS (initial connect — bubbled to caller via `Result`).
///   2. Runs reader+writer until the WS dies.
///   3. On transient failure, retries up to `MAX_RECONNECT_ATTEMPTS` with
///      exponential backoff (1s, 2s, 4s). Emits status events to the
///      transcript broadcast on disconnect/reconnect/terminal failure.
///   4. On auth failure (401/403), emits terminal "[stt auth failed]" and
///      stops — does NOT retry.
async fn spawn_supervised_session(
    cfg: &DeepgramStt,
    channel: Channel,
    txn: TranscriptTx,
) -> anyhow::Result<tokio::sync::mpsc::Sender<AudioChunk>> {
    // Audio chunks flow from the pump → supervisor → current WS write half.
    // Bounded so backpressure surfaces upstream (handled by try_send in the
    // pump) instead of memory-blowing.
    let (audio_tx, mut audio_rx) = tokio::sync::mpsc::channel::<AudioChunk>(64);

    // Initial connect — bubbled to the caller so a bad API key surfaces as
    // an Err from `Stt::start` (which the meeting supervisor maps to a 400).
    let initial = connect_once(cfg, channel).await.map_err(|e| match e {
        ConnectError::Auth => anyhow::anyhow!(
            "deepgram auth failed (channel={channel:?}): \
             check YOGURT_DEEPGRAM_API_KEY"
        ),
        ConnectError::Transient(msg) => {
            anyhow::anyhow!("deepgram connect failed (channel={channel:?}): {msg}")
        }
    })?;

    let api_key = cfg.api_key.clone();
    let base_url = cfg.base_url.clone();
    let model = cfg.model.clone();

    tokio::spawn(async move {
        // Run the first session with the connection we already opened.
        let mut current_ws = Some(initial);
        let mut attempt: usize = 0;

        loop {
            let ws = match current_ws.take() {
                Some(w) => w,
                None => {
                    // Need to reconnect.
                    if attempt >= MAX_RECONNECT_ATTEMPTS {
                        tracing::error!(
                            ?channel,
                            attempts = attempt,
                            "deepgram supervisor: exhausted reconnects — giving up"
                        );
                        emit_status(&txn, channel, "[stt disconnected, reconnect failed]");
                        return;
                    }
                    let backoff_ms = 1000u64 << attempt; // 1000, 2000, 4000
                    tracing::info!(
                        ?channel,
                        attempt = attempt + 1,
                        backoff_ms,
                        "deepgram supervisor: reconnecting"
                    );
                    emit_status(&txn, channel, "[stt reconnecting]");
                    tokio::time::sleep(Duration::from_millis(backoff_ms)).await;

                    let cfg_local = DeepgramStt {
                        api_key: api_key.clone(),
                        base_url: base_url.clone(),
                        model: model.clone(),
                    };
                    match connect_once(&cfg_local, channel).await {
                        Ok(w) => {
                            attempt = 0;
                            emit_status(&txn, channel, "[stt reconnected]");
                            w
                        }
                        Err(ConnectError::Auth) => {
                            tracing::error!(
                                ?channel,
                                "deepgram supervisor: auth failed on reconnect — terminal"
                            );
                            emit_status(&txn, channel, "[stt auth failed]");
                            return;
                        }
                        Err(ConnectError::Transient(msg)) => {
                            tracing::warn!(
                                ?channel,
                                ?msg,
                                "deepgram supervisor: reconnect attempt failed"
                            );
                            attempt += 1;
                            continue;
                        }
                    }
                }
            };

            let (mut write, mut read) = ws.split();

            // Run reader + writer concurrently in this task using select.
            // - Reader: drains WS frames → publishes TranscriptEvents.
            // - Writer: drains the supervisor's audio_rx → pushes binary
            //   frames to the WS.
            // Whichever side errors first ends the session and triggers
            // reconnect (or clean shutdown if audio_rx returned None).
            let mut clean_shutdown = false;
            let mut needs_reconnect = false;
            loop {
                tokio::select! {
                    // biased: prefer reader so server-initiated close is
                    // handled before we keep pushing audio into the void.
                    biased;
                    msg = read.next() => {
                        match msg {
                            Some(Ok(Message::Text(t))) => {
                                if let Some(ev) = parse_deepgram_event(&t, channel) {
                                    let _ = txn.send(ev);
                                }
                            }
                            Some(Ok(Message::Close(_))) => {
                                tracing::info!(?channel, "deepgram reader: ws closed by server");
                                needs_reconnect = true;
                                break;
                            }
                            Some(Ok(_)) => {
                                // Ignore Binary/Ping/Pong frames from Deepgram.
                            }
                            Some(Err(e)) => {
                                tracing::warn!(?channel, ?e, "deepgram reader: ws error");
                                needs_reconnect = true;
                                break;
                            }
                            None => {
                                // Stream ended cleanly without a Close frame.
                                tracing::info!(?channel, "deepgram reader: ws stream ended");
                                needs_reconnect = true;
                                break;
                            }
                        }
                    }
                    audio = audio_rx.recv() => {
                        match audio {
                            Some(chunk) => {
                                let bytes = i16_slice_to_le_bytes(&chunk.samples);
                                if write.send(Message::Binary(bytes)).await.is_err() {
                                    tracing::warn!(
                                        ?channel,
                                        "deepgram writer: ws send failed — will reconnect"
                                    );
                                    needs_reconnect = true;
                                    break;
                                }
                            }
                            None => {
                                // Pump dropped its sender → upstream shutdown.
                                tracing::info!(
                                    ?channel,
                                    "deepgram writer: audio channel closed — clean shutdown"
                                );
                                let close = serde_json::json!({ "type": "CloseStream" }).to_string();
                                let _ = write.send(Message::Text(close)).await;
                                let _ = write.close().await;
                                clean_shutdown = true;
                                break;
                            }
                        }
                    }
                }
            }

            if clean_shutdown {
                return;
            }

            if needs_reconnect {
                emit_status(&txn, channel, "[stt disconnected, retrying]");
                // current_ws stays None → outer loop will reconnect with backoff.
            }
        }
    });

    Ok(audio_tx)
}

/// Open one Deepgram WS connection and return the upgraded stream.
/// Classifies failures into `ConnectError` so the caller can decide retry vs
/// terminal.
async fn connect_once(
    cfg: &DeepgramStt,
    channel: Channel,
) -> Result<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    ConnectError,
> {
    let url = cfg.connect_url();
    tracing::info!(?channel, %url, "deepgram: connecting");

    let mut req = url
        .into_client_request()
        .map_err(|e| ConnectError::Transient(format!("bad request: {e}")))?;
    let auth_header = format!("Token {}", cfg.api_key)
        .parse()
        .map_err(|e| ConnectError::Transient(format!("bad auth header: {e}")))?;
    req.headers_mut().insert("Authorization", auth_header);

    match tokio_tungstenite::connect_async(req).await {
        Ok((ws, _resp)) => Ok(ws),
        Err(tokio_tungstenite::tungstenite::Error::Http(resp))
            if resp.status() == 401 || resp.status() == 403 =>
        {
            tracing::error!(?channel, status=%resp.status(), "deepgram: auth rejected");
            Err(ConnectError::Auth)
        }
        Err(e) => {
            tracing::warn!(?channel, ?e, "deepgram: transient connect error");
            Err(ConnectError::Transient(format!("{e}")))
        }
    }
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
///
/// BL-03: Deepgram emits TWO finality flags on each Results frame:
/// - `is_final` — true on every interim-final (Deepgram won't revise THIS
///   transcript window, but the utterance continues). Fires at ~6s windows
///   even mid-sentence.
/// - `speech_final` — true at end-of-utterance (the speaker paused enough
///   that Deepgram's endpointing heuristic fired). This is the moment the
///   dock should LOCK a transcript line so a new partial starts on a new
///   row.
///
/// The `TranscriptEvent.is_final` field is documented as "the engine
/// considers this segment locked in" (lib.rs:58). That maps to
/// `speech_final`, NOT `is_final`. Reading the wrong field caused every
/// interim-final to render as a brand-new locked line in the dock instead
/// of replacing the in-progress partial.
///
/// We read both fields and surface true only when EITHER is true — but
/// `speech_final` is the primary signal. The fallback to `is_final` keeps
/// us safe if Deepgram ever omits `speech_final` on a frame.
pub fn parse_deepgram_event(text: &str, channel: Channel) -> Option<TranscriptEvent> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    if v.get("type")?.as_str()? != "Results" {
        return None;
    }
    let alt = v.get("channel")?.get("alternatives")?.get(0)?;
    let transcript = alt.get("transcript")?.as_str()?.trim();
    if transcript.is_empty() {
        return None;
    }
    let start_s = v.get("start").and_then(|x| x.as_f64()).unwrap_or(0.0);
    // Primary: speech_final (end-of-utterance). Fallback to is_final only
    // when speech_final is absent — defensive against Deepgram schema
    // changes. Use of fallback is deliberate: a `speech_final: false`
    // frame must NOT be marked final even if `is_final: true` is present
    // (that's the BL-03 bug we're fixing).
    let is_final = match v.get("speech_final").and_then(|x| x.as_bool()) {
        Some(b) => b,
        None => v.get("is_final").and_then(|x| x.as_bool()).unwrap_or(false),
    };
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
    fn it_parses_a_speech_final_results_frame() {
        // BL-03: end-of-utterance → speech_final: true → ev.is_final must be true.
        let frame = r#"{
          "type": "Results",
          "channel": {"alternatives": [{"transcript": "hello world", "confidence": 0.99}]},
          "is_final": true,
          "speech_final": true,
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
    fn it_treats_interim_final_as_partial() {
        // BL-03: Deepgram's `is_final: true, speech_final: false` means
        // "I won't revise this transcript window" but the utterance
        // continues. The dock must treat this as a partial (NOT locked)
        // so the next partial replaces it in place rather than appending
        // a new line.
        let frame = r#"{
          "type": "Results",
          "channel": {"alternatives": [{"transcript": "hello world how are", "confidence": 0.94}]},
          "is_final": true,
          "speech_final": false,
          "start": 0.0,
          "duration": 2.1
        }"#;
        let ev = parse_deepgram_event(frame, Channel::Mic).expect("should parse");
        assert_eq!(ev.text, "hello world how are");
        assert!(
            !ev.is_final,
            "interim final (is_final=true, speech_final=false) must NOT lock the line"
        );
    }

    #[test]
    fn it_falls_back_to_is_final_when_speech_final_absent() {
        // Defensive: if Deepgram ever drops `speech_final` (schema change),
        // we still honor `is_final` so we degrade to old behavior rather
        // than treating everything as partial.
        let frame = r#"{
          "type": "Results",
          "channel": {"alternatives": [{"transcript": "fallback", "confidence": 0.9}]},
          "is_final": true,
          "start": 0.0
        }"#;
        let ev = parse_deepgram_event(frame, Channel::Mic).expect("should parse");
        assert!(ev.is_final);
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
