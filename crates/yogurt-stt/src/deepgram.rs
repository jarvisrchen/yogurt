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
            // nova-3 is markedly better than nova-2 on far-field / noisy
            // audio (observed live: nova-2 silently dropped whole clauses
            // of room audio that other engines caught - it refuses to
            // guess below its confidence floor). Env-overridable for
            // experiments without a rebuild.
            model: std::env::var("YOGURT_DEEPGRAM_MODEL").unwrap_or_else(|_| "nova-3".into()),
        }
    }

    /// Build the connect URL with all query params baked in.
    ///
    /// `endpointing` is the silence (ms) after which Deepgram closes an
    /// utterance — each closed utterance becomes one transcript line. At
    /// the old 300ms every breath and hesitation started a new line
    /// ("...where the rollout." / "We are."), so lines read as chopped
    /// fragments. 1000ms tracks real sentence boundaries; perceived
    /// latency is unchanged because interim partials still stream live.
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
            // Captured before `chunk` is moved into `try_send` below — this
            // is the real monotonic timestamp of the chunk that (if it
            // drops) triggers the overload status event, so the synthetic
            // event lands at the right point on the transcript timeline
            // instead of ts_ms=0.
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
///
/// `ts_ms` should be the most recent real audio timestamp the caller has
/// seen (see `spawn_supervised_session`'s `last_ts_ms` and the pump loop's
/// per-chunk `ts_ms` above) so the synthetic line lands at roughly the
/// right point on the transcript timeline instead of always sorting first
/// at ts_ms=0. Callers with no meaningful position yet (e.g. before any
/// audio has flowed) pass `0`, which is the same behavior this file always
/// had.
fn emit_status(txn: &TranscriptTx, channel: Channel, ts_ms: u64, text: &str) {
    let _ = txn.send(TranscriptEvent {
        ts_ms,
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
        // Real audio timestamp of the last chunk this session forwarded,
        // updated in the writer arm of the select loop below. Survives
        // across reconnects (declared outside the outer `loop`) so a
        // reconnect/auth-failure status event lands near where the audio
        // actually was, instead of always sorting first at ts_ms=0. Stays
        // 0 until the first chunk is ever written — there's no meaningful
        // "last seen" position before that.
        let mut last_ts_ms: u64 = 0;

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
                        emit_status(
                            &txn,
                            channel,
                            last_ts_ms,
                            "[stt disconnected, reconnect failed]",
                        );
                        return;
                    }
                    let backoff_ms = 1000u64 << attempt; // 1000, 2000, 4000
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

            // Run reader + writer concurrently in this task using select.
            // - Reader: drains WS frames → publishes TranscriptEvents.
            // - Writer: drains the supervisor's audio_rx → pushes binary
            //   frames to the WS.
            // Whichever side errors first ends the session and triggers
            // reconnect (or clean shutdown if audio_rx returned None).
            let mut clean_shutdown = false;
            let mut needs_reconnect = false;
            let mut utterance = UtteranceState::default();
            loop {
                tokio::select! {
                    // biased: prefer reader so server-initiated close is
                    // handled before we keep pushing audio into the void.
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
                emit_status(&txn, channel, last_ts_ms, "[stt disconnected, retrying]");
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

/// Per-connection utterance accumulator.
///
/// Deepgram delivers one utterance as MULTIPLE result windows: interim
/// frames (`is_final: false`), then a window-final (`is_final: true,
/// speech_final: false`) that locks THAT WINDOW's text while the utterance
/// continues, and finally `speech_final: true` at end-of-utterance. A
/// window-final's frame carries only its own window's words - so treating
/// frames statelessly loses every earlier window: the dock showed each
/// window replacing the last, and the persisted "final" contained only the
/// utterance's tail (observed live once endpointing went to 1000ms and
/// utterances started spanning windows: a 15-minute meeting persisted a
/// handful of tails).
#[derive(Default)]
pub struct UtteranceState {
    /// Text of window-finalized chunks of the in-progress utterance.
    buf: String,
    /// Timestamp of the utterance's FIRST audio, carried onto every event
    /// so dock rows and deep-links anchor at the utterance start.
    start_ms: Option<u64>,
}

fn join_words(a: &str, b: &str) -> String {
    match (a.is_empty(), b.is_empty()) {
        (true, _) => b.to_string(),
        (_, true) => a.to_string(),
        _ => format!("{a} {b}"),
    }
}

/// Fold one Deepgram JSON frame into `st`, returning the event to emit:
/// - interim         -> partial carrying buffered + live window text
/// - window-final    -> partial carrying the full buffered text so far
/// - speech_final    -> FINAL carrying the complete joined utterance; state resets
pub fn fold_deepgram_event(
    st: &mut UtteranceState,
    text: &str,
    channel: Channel,
) -> Option<TranscriptEvent> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    if v.get("type")?.as_str()? != "Results" {
        return None;
    }
    let transcript = v
        .get("channel")?
        .get("alternatives")?
        .get(0)?
        .get("transcript")?
        .as_str()?
        .trim()
        .to_string();
    let window_final = v.get("is_final").and_then(|x| x.as_bool()).unwrap_or(false);
    let speech_final = v
        .get("speech_final")
        .and_then(|x| x.as_bool())
        .unwrap_or(window_final);
    let ts_now = (v.get("start").and_then(|x| x.as_f64()).unwrap_or(0.0) * 1000.0) as u64;

    if transcript.is_empty() && !speech_final {
        return None; // silence/heartbeat frame
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
        Some(TranscriptEvent {
            ts_ms,
            channel,
            text: full,
            is_final: true,
        })
    } else if window_final {
        st.buf = join_words(&st.buf, &transcript);
        Some(TranscriptEvent {
            ts_ms,
            channel,
            text: st.buf.clone(),
            is_final: false,
        })
    } else {
        Some(TranscriptEvent {
            ts_ms,
            channel,
            text: join_words(&st.buf, &transcript),
            is_final: false,
        })
    }
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

    /// `emit_status` must carry through whatever `ts_ms` the caller passes
    /// rather than hardcoding 0 — callers now thread the real last-seen
    /// audio timestamp (`spawn_supervised_session`'s `last_ts_ms`, or the
    /// dropped chunk's own `ts_ms` in the backpressure path) so synthetic
    /// status lines land at the right point on the transcript timeline.
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

    /// The bug observed live: an utterance spanning two Deepgram windows
    /// must persist ALL its text, not just the tail window.
    #[test]
    fn multi_window_utterance_joins_all_text() {
        let mut st = UtteranceState::default();
        // interim, then window-final locking window 1
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
        // next window's interim must carry the buffer
        let p2 = fold_deepgram_event(
            &mut st,
            &frame("from the changes", 9.5, false, false),
            Channel::Mic,
        )
        .unwrap();
        assert_eq!(p2.text, "we wanna isolate them longer from the changes");
        assert_eq!(p2.ts_ms, 4000, "anchored at utterance start");
        // speech_final closes with the FULL joined text
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
        // state reset: next utterance starts fresh
        let n = fold_deepgram_event(
            &mut st,
            &frame("next thought", 12.0, false, false),
            Channel::Mic,
        )
        .unwrap();
        assert_eq!(n.text, "next thought");
        assert_eq!(n.ts_ms, 12000);
    }

    /// Deepgram may close an utterance with an EMPTY speech_final frame -
    /// the buffered windows must still flush as the final.
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

    /// Single-window utterances (short speech) behave exactly as before.
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
