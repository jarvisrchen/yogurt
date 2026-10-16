//! Deepgram streaming adapter for `wss://api.deepgram.com/v1/listen`.
//!
//! Audio goes in as binary frames of 16 kHz mono linear16 PCM, events come back as JSON `Results` frames.
//!
//! Each `Stt::start` opens one WS per [`Channel`], so a "Me" line is never tagged "Them".
//! That costs 2x Deepgram seconds but preserves the channel label without diarization.
//!
//! Reliability:
//!
//! - Backpressure: the per-channel mpsc uses `try_send`, so a stalled WS never blocks the other channel.
//!   `MAX_CONSECUTIVE_DROPS` drops in a row emit a synthetic overload event.
//! - Reconnect: transient errors retry up to `MAX_RECONNECT_ATTEMPTS` times with 1s, 2s, 4s backoff and a synthetic status event.
//! - Auth: HTTP 401/403 on the upgrade is terminal, with an "[stt auth failed]" event so the user can fix the key.

use crate::{AudioChunk, AudioRx, Channel, Stt, TranscriptEvent, TranscriptTx};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use std::time::Duration;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;

/// Consecutive `try_send` failures before a channel session is declared unrecoverable (about 1 s of audio at ~50 fps).
const MAX_CONSECUTIVE_DROPS: usize = 50;

/// Maximum reconnect attempts after the initial connect succeeds.
const MAX_RECONNECT_ATTEMPTS: usize = 3;

pub struct DeepgramStt {
    pub api_key: String,
    /// Overrides the WS base URL (used by tests).
    pub base_url: String,
    pub model: String,
}

impl DeepgramStt {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: "wss://api.deepgram.com".into(),
            // nova-3 handles far-field and noisy audio better: nova-2 silently drops clauses below its confidence floor.
            model: std::env::var("YOGURT_DEEPGRAM_MODEL").unwrap_or_else(|_| "nova-3".into()),
        }
    }

    /// Build the connect URL with all query params baked in.
    ///
    /// `endpointing` is the silence in ms that closes an utterance into one transcript line.
    /// At 300 ms every breath started a new line, while 1000 ms tracks sentence boundaries and interim partials still stream live.
    fn connect_url(&self) -> String {
        format!(
            "{base}/v1/listen?model={model}\
             &encoding=linear16&sample_rate=16000&channels=1\
             &interim_results=true&endpointing=1000&smart_format=true",
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
        let mic = spawn_supervised_session(self, Channel::Mic, txn.clone()).await?;
        let sys = spawn_supervised_session(self, Channel::System, txn.clone()).await?;

        // `try_send` so a stalled WS doesn't block the other channel.
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
            // Taken before `chunk` moves, so a synthetic overload event lands at the right point on the timeline.
            let ts_ms = chunk.ts_ms;

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
                        emit_status(
                            &txn,
                            channel,
                            ts_ms,
                            "[stt overloaded, transcript may be lossy]",
                        );
                        // Reset to avoid spamming; the supervisor's reconnect path signals recovery.
                        *drops = 0;
                    }
                }
                Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                    tracing::warn!(
                        ?channel,
                        "deepgram pump: session task gone (channel closed)"
                    );
                    // Keep going: the other channel may still be healthy.
                }
            }
        }

        drop(mic);
        drop(sys);
        Ok(())
    }
}

/// Emit a synthetic status event, final so the dock renders it as a locked line.
///
/// `ts_ms` is the latest real audio timestamp the caller has seen, so the line sorts near where the audio was. Pass `0` before any audio has flowed.
fn emit_status(txn: &TranscriptTx, channel: Channel, ts_ms: u64, text: &str) {
    let _ = txn.send(TranscriptEvent {
        ts_ms,
        channel,
        text: text.to_string(),
        is_final: true,
        confidence: None,
    });
}

/// Open a self-healing Deepgram session for one channel and return the sender the audio pump feeds.
///
/// The initial connect error is returned to the caller. A supervisor task handles reconnects and status events afterwards.
async fn spawn_supervised_session(
    cfg: &DeepgramStt,
    channel: Channel,
    txn: TranscriptTx,
) -> anyhow::Result<tokio::sync::mpsc::Sender<AudioChunk>> {
    // Bounded so backpressure surfaces at the pump's `try_send` instead of growing memory.
    let (audio_tx, mut audio_rx) = tokio::sync::mpsc::channel::<AudioChunk>(64);

    // A bad API key surfaces here as an `Err` from `Stt::start`.
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
        let mut current_ws = Some(initial);
        let mut attempt: usize = 0;
        // Audio timestamp of the last forwarded chunk, kept across reconnects so status events land near the audio.
        let mut last_ts_ms: u64 = 0;

        loop {
            let ws = match current_ws.take() {
                Some(w) => w,
                None => {
                    if attempt >= MAX_RECONNECT_ATTEMPTS {
                        tracing::error!(
                            ?channel,
                            attempts = attempt,
                            "deepgram supervisor: exhausted reconnects — giving up"
                        );
                        emit_status(
                            &txn,
                            channel,
                            last_ts_ms,
                            "[stt disconnected, reconnect failed]",
                        );
                        return;
                    }
                    let backoff_ms = 1000u64 << attempt;
                    tracing::info!(
                        ?channel,
                        attempt = attempt + 1,
                        backoff_ms,
                        "deepgram supervisor: reconnecting"
                    );
                    emit_status(&txn, channel, last_ts_ms, "[stt reconnecting]");
                    tokio::time::sleep(Duration::from_millis(backoff_ms)).await;

                    let cfg_local = DeepgramStt {
                        api_key: api_key.clone(),
                        base_url: base_url.clone(),
                        model: model.clone(),
                    };
                    match connect_once(&cfg_local, channel).await {
                        Ok(w) => {
                            attempt = 0;
                            emit_status(&txn, channel, last_ts_ms, "[stt reconnected]");
                            w
                        }
                        Err(ConnectError::Auth) => {
                            tracing::error!(
                                ?channel,
                                "deepgram supervisor: auth failed on reconnect — terminal"
                            );
                            emit_status(&txn, channel, last_ts_ms, "[stt auth failed]");
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

            // Reader and writer run in one select. The first side to error ends the session and triggers a reconnect.
            let mut clean_shutdown = false;
            let mut needs_reconnect = false;
            let mut utterance = UtteranceState::default();
            loop {
                tokio::select! {
                    // Prefer the reader so a server-initiated close is handled before audio is pushed into the void.
                    biased;
                    msg = read.next() => {
                        match msg {
                            Some(Ok(Message::Text(t))) => {
                                if let Some(ev) = fold_deepgram_event(&mut utterance, &t, channel) {
                                    let _ = txn.send(ev);
                                }
                            }
                            Some(Ok(Message::Close(_))) => {
                                tracing::info!(?channel, "deepgram reader: ws closed by server");
                                needs_reconnect = true;
                                break;
                            }
                            Some(Ok(_)) => {
                            }
                            Some(Err(e)) => {
                                tracing::warn!(?channel, ?e, "deepgram reader: ws error");
                                needs_reconnect = true;
                                break;
                            }
                            None => {
                                tracing::info!(?channel, "deepgram reader: ws stream ended");
                                needs_reconnect = true;
                                break;
                            }
                        }
                    }
                    audio = audio_rx.recv() => {
                        match audio {
                            Some(chunk) => {
                                last_ts_ms = chunk.ts_ms;
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
                emit_status(&txn, channel, last_ts_ms, "[stt disconnected, retrying]");
            }
        }
    });

    Ok(audio_tx)
}

/// Open one Deepgram WS connection, classifying failures as retryable or terminal.
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

/// Per-connection utterance accumulator.
///
/// Deepgram delivers one utterance as several result windows: interim frames, then window-finals (`is_final: true, speech_final: false`)
/// that carry only their own window's words, and finally `speech_final: true`.
/// Treating frames statelessly loses every earlier window, so the persisted final held only the utterance's tail.
#[derive(Default)]
pub struct UtteranceState {
    /// Text of window-finalized chunks of the in-progress utterance.
    buf: String,
    /// Timestamp of the utterance's first audio, carried onto every event.
    start_ms: Option<u64>,
    /// Text of the last final emitted on this connection.
    /// Deepgram can resend the terminal `speech_final` frame on reconnect, and by then `buf` and `start_ms` have been reset,
    /// so the duplicate would re-emit as a new utterance. Comparing against this drops the repeat.
    last_final: Option<String>,
}

fn join_words(a: &str, b: &str) -> String {
    match (a.is_empty(), b.is_empty()) {
        (true, _) => b.to_string(),
        (_, true) => a.to_string(),
        _ => format!("{a} {b}"),
    }
}

/// Fold one Deepgram JSON frame into `st`, returning the event to emit:
/// - interim -> partial with buffered plus live window text
/// - window-final -> partial with the full buffered text
/// - speech_final -> final with the complete joined utterance, then state resets
pub fn fold_deepgram_event(
    st: &mut UtteranceState,
    text: &str,
    channel: Channel,
) -> Option<TranscriptEvent> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    if v.get("type")?.as_str()? != "Results" {
        return None;
    }
    let alt = v.get("channel")?.get("alternatives")?.get(0)?;
    let transcript = alt.get("transcript")?.as_str()?.trim().to_string();
    let confidence = alt
        .get("confidence")
        .and_then(|x| x.as_f64())
        .map(|c| c as f32);
    let window_final = v.get("is_final").and_then(|x| x.as_bool()).unwrap_or(false);
    let speech_final = v
        .get("speech_final")
        .and_then(|x| x.as_bool())
        .unwrap_or(window_final);
    let ts_now = (v.get("start").and_then(|x| x.as_f64()).unwrap_or(0.0) * 1000.0) as u64;

    if transcript.is_empty() && !speech_final {
        return None;
    }
    if st.start_ms.is_none() && !transcript.is_empty() {
        st.start_ms = Some(ts_now);
    }
    let ts_ms = st.start_ms.unwrap_or(ts_now);

    if speech_final {
        let full = join_words(&st.buf, &transcript);
        st.buf.clear();
        st.start_ms = None;
        if full.is_empty() {
            return None;
        }
        if st.last_final.as_deref() == Some(full.as_str()) {
            // Immediate repeat of the final just emitted (resent frame or duplicate empty `speech_final`).
            return None;
        }
        st.last_final = Some(full.clone());
        Some(TranscriptEvent {
            ts_ms,
            channel,
            text: full,
            is_final: true,
            confidence,
        })
    } else if window_final {
        st.buf = join_words(&st.buf, &transcript);
        Some(TranscriptEvent {
            ts_ms,
            channel,
            text: st.buf.clone(),
            is_final: false,
            confidence,
        })
    } else {
        Some(TranscriptEvent {
            ts_ms,
            channel,
            text: join_words(&st.buf, &transcript),
            is_final: false,
            confidence,
        })
    }
}

/// Parse one Deepgram JSON frame into a [`TranscriptEvent`], or `None` for non-`Results` frames and empty transcripts.
///
/// `speech_final` marks end-of-utterance and is what locks a line.
/// `is_final` fires per ~6 s window mid-sentence, so reading it alone made every window render as a new locked line.
/// It is used only when `speech_final` is absent.
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
    let confidence = alt
        .get("confidence")
        .and_then(|x| x.as_f64())
        .map(|c| c as f32);
    let start_s = v.get("start").and_then(|x| x.as_f64()).unwrap_or(0.0);
    let is_final = match v.get("speech_final").and_then(|x| x.as_bool()) {
        Some(b) => b,
        None => v.get("is_final").and_then(|x| x.as_bool()).unwrap_or(false),
    };
    Some(TranscriptEvent {
        ts_ms: (start_s * 1000.0) as u64,
        channel,
        text: transcript.to_string(),
        is_final,
        confidence,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_parses_a_speech_final_results_frame() {
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
        assert_eq!(ev.confidence, Some(0.99));
    }

    #[test]
    fn it_treats_interim_final_as_partial() {
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
        assert_eq!(bytes, vec![0x00, 0x00, 0x01, 0x00, 0xff, 0xff, 0x00, 0x01]);
    }

    /// `emit_status` must carry through the caller's `ts_ms` rather than hardcoding 0.
    #[test]
    fn emit_status_carries_the_caller_supplied_timestamp() {
        let (tx, mut rx) = tokio::sync::broadcast::channel(4);
        emit_status(&tx, Channel::Mic, 42_000, "[stt reconnecting]");
        let ev = rx.try_recv().expect("event was sent");
        assert_eq!(ev.ts_ms, 42_000);
        assert_eq!(ev.channel, Channel::Mic);
        assert!(ev.is_final, "status events must be final/locked lines");
        assert_eq!(ev.text, "[stt reconnecting]");
    }
}

#[cfg(test)]
mod utterance_tests {
    use super::*;

    fn frame(text: &str, start: f64, window_final: bool, speech_final: bool) -> String {
        format!(
            r#"{{"type":"Results","channel":{{"alternatives":[{{"transcript":"{text}"}}]}},"is_final":{window_final},"speech_final":{speech_final},"start":{start}}}"#
        )
    }

    /// An utterance spanning two Deepgram windows must persist all its text, not just the tail.
    #[test]
    fn multi_window_utterance_joins_all_text() {
        let mut st = UtteranceState::default();
        let p1 = fold_deepgram_event(
            &mut st,
            &frame("we wanna isolate", 4.0, false, false),
            Channel::Mic,
        )
        .unwrap();
        assert!(!p1.is_final);
        let w1 = fold_deepgram_event(
            &mut st,
            &frame("we wanna isolate them longer", 4.0, true, false),
            Channel::Mic,
        )
        .unwrap();
        assert!(!w1.is_final, "window-final is not utterance-final");
        assert_eq!(w1.text, "we wanna isolate them longer");
        let p2 = fold_deepgram_event(
            &mut st,
            &frame("from the changes", 9.5, false, false),
            Channel::Mic,
        )
        .unwrap();
        assert_eq!(p2.text, "we wanna isolate them longer from the changes");
        assert_eq!(p2.ts_ms, 4000, "anchored at utterance start");
        let f = fold_deepgram_event(
            &mut st,
            &frame("from the changes we introduce", 9.5, true, true),
            Channel::Mic,
        )
        .unwrap();
        assert!(f.is_final);
        assert_eq!(
            f.text,
            "we wanna isolate them longer from the changes we introduce"
        );
        assert_eq!(f.ts_ms, 4000);
        let n = fold_deepgram_event(
            &mut st,
            &frame("next thought", 12.0, false, false),
            Channel::Mic,
        )
        .unwrap();
        assert_eq!(n.text, "next thought");
        assert_eq!(n.ts_ms, 12000);
    }

    /// An empty `speech_final` frame must still flush the buffered windows as the final.
    #[test]
    fn empty_speech_final_flushes_buffer() {
        let mut st = UtteranceState::default();
        fold_deepgram_event(
            &mut st,
            &frame("hello there", 1.0, true, false),
            Channel::System,
        )
        .unwrap();
        let f = fold_deepgram_event(&mut st, &frame("", 2.0, true, true), Channel::System).unwrap();
        assert!(f.is_final);
        assert_eq!(f.text, "hello there");
        assert_eq!(f.ts_ms, 1000);
    }

    /// A resent identical `speech_final` frame (e.g. on reconnect) must not emit the same final twice.
    #[test]
    fn duplicate_speech_final_frame_is_suppressed() {
        let mut st = UtteranceState::default();
        let f1 = fold_deepgram_event(
            &mut st,
            &frame("hello there", 4.0, true, true),
            Channel::Mic,
        )
        .unwrap();
        assert!(f1.is_final);
        assert_eq!(f1.text, "hello there");
        assert_eq!(f1.ts_ms, 4000);

        let f2 = fold_deepgram_event(
            &mut st,
            &frame("hello there", 4.0, true, true),
            Channel::Mic,
        );
        assert!(
            f2.is_none(),
            "resent speech_final frame must not emit a second final"
        );
    }

    /// A duplicate `speech_final` with `start` 0 after the reset must be suppressed, not re-emitted at ts_ms 0.
    #[test]
    fn duplicate_speech_final_with_zero_start_after_reset_is_suppressed() {
        let mut st = UtteranceState::default();
        fold_deepgram_event(
            &mut st,
            &frame("hello there", 4.0, true, true),
            Channel::Mic,
        )
        .unwrap();

        let f2 = fold_deepgram_event(
            &mut st,
            &frame("hello there", 0.0, true, true),
            Channel::Mic,
        );
        assert!(
            f2.is_none(),
            "late duplicate final with no start info must not re-emit at ts_ms 0"
        );
    }

    /// A new utterance with different text after a final must still emit.
    #[test]
    fn distinct_final_after_a_final_still_emits() {
        let mut st = UtteranceState::default();
        fold_deepgram_event(
            &mut st,
            &frame("hello there", 4.0, true, true),
            Channel::Mic,
        )
        .unwrap();

        let f2 = fold_deepgram_event(
            &mut st,
            &frame("a different sentence", 6.0, true, true),
            Channel::Mic,
        )
        .unwrap();
        assert!(f2.is_final);
        assert_eq!(f2.text, "a different sentence");
        assert_eq!(f2.ts_ms, 6000);
    }

    /// A single-window utterance finalizes directly.
    #[test]
    fn single_window_utterance_is_unchanged() {
        let mut st = UtteranceState::default();
        let f = fold_deepgram_event(
            &mut st,
            &frame("quick reply", 3.0, true, true),
            Channel::Mic,
        )
        .unwrap();
        assert!(f.is_final);
        assert_eq!(f.text, "quick reply");
        assert!(
            fold_deepgram_event(&mut st, &frame("", 4.0, false, false), Channel::Mic).is_none()
        );
    }
}
