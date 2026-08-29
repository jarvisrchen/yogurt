# Yogurt v1 — Phase 3: Cloud STT (Deepgram streaming) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire the Phase-2 audio broadcast into Deepgram's streaming WebSocket, fan the resulting partial + final transcripts to browser clients via a NEW server WebSocket route `/ws/meetings/:id`, and ship the right-edge collapsible "Live transcript" dock panel on the frontend — collapsed-by-default tab, 340ms `cubic-bezier(.2,.7,.2,1)` slide-in, channel-labeled "Me"/"Them" lines with JetBrains-Mono timestamps. The notes column stays editable while the dock is open. By the end of this phase the user can `cargo run -p yogurt -- start`, open a meeting, hit Start, talk into their mic, and watch transcript lines stream into the browser with < 2s lag.

**Architecture:** A new `yogurt-stt` crate defines an `Stt` trait (consumes audio, emits `TranscriptEvent`s) and a Deepgram streaming adapter built on `tokio-tungstenite`. `yogurt-server` gets two new modules — `meetings.rs` (in-memory `Meeting` registry that wires `yogurt-audio` → `yogurt-stt`) and `ws.rs` (axum WebSocket extractor that subscribes to the meeting's transcript broadcast and pushes JSON frames to the browser). Three new REST endpoints — `POST /api/meetings`, `POST /api/meetings/:id/start`, `POST /api/meetings/:id/stop` — gate the audio + STT lifecycle. The frontend gains a `Meeting.tsx` route with a notes column (TipTap stub from Phase 0) plus a `TranscriptDock` component that connects via native `WebSocket` and animates in/out via a Tailwind 4 `@keyframes` block in `index.css`.

**Tech Stack additions (on top of Phase 0–2):** `async-trait 0.1` · `tokio-tungstenite 0.24` (with `rustls-tls-webpki-roots` feature) · `futures-util 0.3` · `url 2` · `uuid 1` (v7, for meeting ids) · `axum::extract::ws` · Deepgram model `nova-2` (multi-language, supports streaming partials) · React 19 `useEffect` + native `WebSocket` API · Tailwind 4 `@keyframes` defined in `web/src/index.css`.

**Reference:** `docs/PRD.md` §3 (users — compliance, OSS, self-hosted-LLM personas all care about transcript latency), §5.2 (live transcript panel — collapsed-by-default tab, 330px wide, "Me"/"Them" channel labels, JetBrains Mono timestamps, `< 2s` lag spec), §10 (WebSocket — `S→C transcript { ts_ms, channel, text, is_final }`), §16.2 (palette tokens — `--ink #211D18`, `--grey #A89F90`, `--line #EBE3D5`), §16.5 (motion — 340ms `cubic-bezier(.2,.7,.2,1)` `slideInRight` for the dock), §3 (single-process architecture — STT lives in-process).

**Out of scope (deferred to later phase plans):**
- Persistence of meetings or transcripts to SQLite (Phase 7 — for now meetings live in a `tokio::sync::RwLock<HashMap<MeetingId, Meeting>>` and vanish on server restart).
- Local `whisper.cpp` adapter (Phase 8 — only the `Stt` trait + Deepgram adapter ship here).
- Settings UI for managing the Deepgram API key (Phase 5 — for now the key is read from `YOGURT_DEEPGRAM_API_KEY` and the server refuses to start a meeting if it's missing).
- AssemblyAI / Groq STT adapters (not in v1 at all; trait is the extension point).
- The TipTap "augmented notes" `aiGrey` mark + transcript-deep-links (Phase 4).
- The Ask-this-meeting chat pill (Phase 6).
- Pause / resume of recording mid-meeting (v1.1 — Start/Stop only).
- Diarization beyond mic/system channel labels (explicit anti-goal per PRD §2 — Granola itself only does "Me"/"Them").
- Audio level meter on the dock tab (just the 3-bar wave animation from Phase 1 design tokens is reused — no real RMS metering wiring this phase).

---

## File structure produced by this phase

```
yogurt/
├── Cargo.toml                                # MODIFY · add yogurt-stt to workspace, add workspace deps
├── crates/
│   ├── yogurt-stt/                           # NEW CRATE
│   │   ├── Cargo.toml                        # NEW
│   │   ├── src/
│   │   │   ├── lib.rs                        # NEW · Stt trait + TranscriptEvent + Channel + TranscriptTx/Rx aliases
│   │   │   └── deepgram.rs                   # NEW · DeepgramStt adapter
│   │   └── tests/
│   │       └── deepgram_mock.rs              # NEW · mock-WS server asserts event mapping
│   └── yogurt-server/
│       ├── Cargo.toml                        # MODIFY · add yogurt-stt, yogurt-audio, uuid, tokio-tungstenite (for tests), axum "ws" feature
│       └── src/
│           ├── lib.rs                        # MODIFY · register meetings::Registry as router state, add ws + meetings modules
│           ├── routes.rs                     # MODIFY · mount /api/meetings POST/start/stop and /ws/meetings/:id
│           ├── meetings.rs                   # NEW · Meeting struct + Registry + start_meeting wiring
│           └── ws.rs                         # NEW · axum WebSocket handler (transcript broadcast → JSON frames)
└── web/
    └── src/
        ├── index.css                         # MODIFY · add @keyframes slideInRight + .dock-* animation utilities
        ├── App.tsx                           # MODIFY · add minimal route table (library stub → Meeting view)
        ├── lib/
        │   └── ws.ts                         # NEW · useTranscriptWs hook (native WebSocket, JSON parse, reconnect)
        ├── components/
        │   ├── TranscriptDock.tsx            # NEW · collapsible right-edge dock (tab + sliding panel)
        │   ├── TranscriptDock.test.tsx       # NEW · Vitest: renders tab collapsed, expands on click, animation class applied
        │   └── TranscriptLine.tsx            # NEW · channel-label + mono timestamp + text (no test of its own; covered via dock test)
        └── routes/
            └── Meeting.tsx                   # NEW · notes column + TranscriptDock + Start/Stop controls
```

**Why these splits:**
- `yogurt-stt` is its own crate because Phase 8 will add a second adapter (`whisper.cpp`) behind the same `Stt` trait — the trait + transcript event types must be shared without dragging in the server's dependency surface.
- `meetings.rs` is separate from `routes.rs` because the in-memory registry needs ownership semantics (`Arc<Registry>`) that don't belong inline with the router. It also makes Phase 7 (SQLite persistence) a swap-in change to one file.
- `ws.rs` is separate from `routes.rs` because WebSocket upgrade handlers have a different shape (`WebSocketUpgrade` extractor + spawned task) than regular handlers, and the file will keep growing in Phase 4 (notes sync) and Phase 6 (chat).
- The `web/src/routes/` folder is introduced here (Phase 0 only had `App.tsx`) — Phase 7 (library + onboarding) will add `Library.tsx`, `Welcome.tsx`, `Settings.tsx`, etc. We keep the routing minimal in Phase 3 (no `react-router`; `App.tsx` switches on a `useState<View>` enum) and defer the router decision to Phase 7.

---

## Test conventions established in this phase

Phase 0 already set the baseline (`#[cfg(test)] mod tests` inline; integration tests in `crates/<crate>/tests/`; Vitest for the web). Phase 3 adds:

- **WebSocket integration tests (server side):** spawn the axum server on a random high port, connect a `tokio-tungstenite` client, drive the lifecycle (`POST /api/meetings` → `POST /api/meetings/:id/start` → push synthetic audio → assert frames on the WS).
- **WebSocket integration tests (client side, Vitest):** mock the global `WebSocket` constructor in the test; assert the `TranscriptDock` re-renders new lines as `onmessage` fires.
- **Mock Deepgram server:** `tokio-tungstenite::accept_async` on a localhost port; the test injects a `DEEPGRAM_BASE_URL` override into the `DeepgramStt` constructor so the adapter dials our mock instead of `wss://api.deepgram.com`. The mock echoes pre-canned Deepgram-shaped JSON responses to assert event mapping.
- **Synthetic audio source for E2E:** a `Vec<i16>` of 1 second of 16 kHz silence (or a 440 Hz sine wave for visual sanity during local dev) is pushed through the `yogurt-audio` broadcast in tests rather than spinning up real ScreenCaptureKit (which requires Screen Recording permission and is not CI-friendly).

**Test ports used (avoid collisions across files):** `17890`, `17891`, `17892` for `yogurt-server` integration tests; the mock-Deepgram server uses `0` (OS-assigned ephemeral, queried back via `local_addr()`).

---

## Phase 3 task list

10 tasks. Each ends with a commit. Approximate sequence: ~12–16 hours of focused work spread across 2 days.

---

### Task 3.1 · Create `yogurt-stt` crate with `Stt` trait + `TranscriptEvent`

**Files:**
- Modify: `Cargo.toml` (workspace) — add `yogurt-stt` to `members`; add `async-trait`, `tokio-tungstenite`, `futures-util`, `url`, `uuid` to `[workspace.dependencies]`.
- Create: `crates/yogurt-stt/Cargo.toml`
- Create: `crates/yogurt-stt/src/lib.rs`

- [ ] **Step 1: Inspect current state.**

Run: `ls crates/` and `cat Cargo.toml | head -20`
Expected: `yogurt-cli/`, `yogurt-server/`, `yogurt-audio/` (added in Phase 2). Workspace lists those three. No `yogurt-stt/` yet.

- [ ] **Step 2: Update workspace `Cargo.toml`.**

Add to `[workspace] members`:
```toml
"crates/yogurt-stt",
```

Append to `[workspace.dependencies]` (alphabetized into the existing block — these are the new lines):
```toml
async-trait = "0.1"
futures-util = { version = "0.3", default-features = false, features = ["sink", "std"] }
tokio-tungstenite = { version = "0.24", features = ["rustls-tls-webpki-roots"] }
url = "2"
uuid = { version = "1", features = ["v7", "serde"] }
```

> **⚠ Note:** `tokio-tungstenite 0.24` requires `tokio 1.42` (already in workspace). The `rustls-tls-webpki-roots` feature avoids needing OpenSSL at compile time — matches the `reqwest` config from Phase 0 (`default-features = false, features = ["rustls-tls"]`). Do not enable the default `native-tls` feature: it pulls OpenSSL and breaks the single-binary distribution story.

- [ ] **Step 3: Write `crates/yogurt-stt/Cargo.toml`.**

```toml
[package]
name = "yogurt-stt"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true
repository.workspace = true
description = "Pluggable speech-to-text trait + adapters for yogurt."

[dependencies]
tokio = { workspace = true }
tokio-tungstenite = { workspace = true }
futures-util = { workspace = true }
async-trait = { workspace = true }
url = { workspace = true }
anyhow = { workspace = true }
tracing = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }

[dev-dependencies]
tokio = { workspace = true }
tokio-tungstenite = { workspace = true }
```

(Notice: `yogurt-stt` does NOT depend on `yogurt-audio`. The trait takes a generic broadcast receiver of `i16` samples + a `Channel` enum tag — keeping the crate dependency-light means Phase 8 can add a `whisper.cpp` adapter without yanking in audio code. The server crate is responsible for wiring `yogurt-audio`'s output into the receiver.)

- [ ] **Step 4: Write `crates/yogurt-stt/src/lib.rs`.**

```rust
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
```

- [ ] **Step 5: Add a stub `crates/yogurt-stt/src/deepgram.rs` (real impl in Task 3.2).**

```rust
//! Deepgram streaming adapter. Real implementation lands in Task 3.2.

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
        anyhow::bail!("yogurt-stt: deepgram adapter not yet implemented (task 3.2)");
    }
}
```

- [ ] **Step 6: Verify the crate compiles.**

Run: `cargo check -p yogurt-stt`
Expected: clean. Two warnings allowed (`unused` on the stub `deepgram` fields) — Task 3.2 uses them.

- [ ] **Step 7: Add the smallest possible smoke test for the trait shape.**

Append to `crates/yogurt-stt/src/lib.rs`:

```rust
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
```

Run: `cargo test -p yogurt-stt`
Expected: `1 passed`.

- [ ] **Step 8: Commit.**

```bash
git add Cargo.toml crates/yogurt-stt/
git commit -m "feat(stt): add yogurt-stt crate with Stt trait + TranscriptEvent"
```

---

### Task 3.2 · Implement the Deepgram streaming adapter

**Files:**
- Modify: `crates/yogurt-stt/src/deepgram.rs` (real impl)

> **Deepgram protocol crash course** (so the implementer doesn't have to context-switch into Deepgram docs mid-task):
>
> 1. **Open:** `wss://api.deepgram.com/v1/listen?model=nova-2&encoding=linear16&sample_rate=16000&channels=1&interim_results=true&endpointing=300` with header `Authorization: Token <API_KEY>`.
> 2. **Send:** raw little-endian i16 PCM samples as **binary** WS frames. Any chunk size is fine.
> 3. **Receive:** JSON text frames of the form
>    ```json
>    {
>      "type": "Results",
>      "channel": {"alternatives": [{"transcript": "hello world", "confidence": 0.99}]},
>      "is_final": true,
>      "start": 1.42,    // seconds from session start
>      "duration": 0.7
>    }
>    ```
>    Also: `{"type": "Metadata", ...}` (ignored) and `{"type": "SpeechStarted", ...}` (ignored).
> 4. **Close:** send a text frame `{"type": "CloseStream"}` then close the WS. Deepgram drains, sends a final `is_final: true` result, then closes.

- [ ] **Step 1: Replace `crates/yogurt-stt/src/deepgram.rs` with the real adapter.**

```rust
//! Deepgram streaming adapter — wss://api.deepgram.com/v1/listen.
//!
//! Wire format reference: https://developers.deepgram.com/docs/streaming
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
            if write.send(Message::Binary(bytes.into())).await.is_err() {
                tracing::warn!(?channel, "deepgram writer: ws send failed");
                return;
            }
        }
        // Channel closed → tell Deepgram we're done so it sends any tail results.
        let close = serde_json::json!({ "type": "CloseStream" }).to_string();
        let _ = write.send(Message::Text(close.into())).await;
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
```

- [ ] **Step 2: Run unit tests — expect PASS.**

Run: `cargo test -p yogurt-stt --lib`
Expected: `5 passed` (the serialize test from Task 3.1 + 4 new in `deepgram::tests`).

- [ ] **Step 3: Commit.**

```bash
git add crates/yogurt-stt/src/deepgram.rs
git commit -m "feat(stt): implement deepgram streaming adapter (nova-2, dual-channel)"
```

---

### Task 3.3 · Mock-Deepgram integration test (proves end-to-end mapping)

**Files:**
- Create: `crates/yogurt-stt/tests/deepgram_mock.rs`

> **Strategy:** Stand up a localhost WS server with `tokio-tungstenite::accept_async`, point `DeepgramStt::base_url` at `ws://127.0.0.1:<port>` (note: `ws://` not `wss://` — the test server is unencrypted, and `tokio-tungstenite` cheerfully handles both). When the adapter dials in, the mock waits for at least one binary frame (proving audio is flowing), then sends a canned Deepgram results frame back. The test asserts the right `TranscriptEvent` lands on the broadcast.

- [ ] **Step 1: Write the test file.**

```rust
//! End-to-end mapping test for the Deepgram adapter using a mock WS server.
//!
//! The mock accepts a connection, drains audio frames, then replies with a
//! hand-rolled "Results" JSON. We assert the adapter publishes the matching
//! TranscriptEvent on the broadcast.

use futures_util::{SinkExt, StreamExt};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;
use yogurt_stt::{deepgram::DeepgramStt, AudioChunk, Channel, Stt, TranscriptEvent};

#[tokio::test(flavor = "multi_thread")]
async fn it_pipes_audio_to_mock_and_emits_transcript_event() {
    // 1. Start a mock WS server on an ephemeral port.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    let server = tokio::spawn(async move {
        // Accept ONE connection (one per channel = mic, then system).
        // For this test we only care about the mic session — accept it,
        // wait for a binary frame, send a canned Results frame, then close.
        // Then accept the system session and immediately close (no transcript).
        for _channel_n in 0..2 {
            let (stream, _) = listener.accept().await.unwrap();
            tokio::spawn(async move {
                let ws = tokio_tungstenite::accept_async(stream).await.unwrap();
                let (mut write, mut read) = ws.split();

                // Drain at least one audio frame to confirm bytes flow.
                let mut got_audio = false;
                while let Some(Ok(msg)) = read.next().await {
                    if matches!(msg, Message::Binary(_)) {
                        got_audio = true;
                        break;
                    }
                    if matches!(msg, Message::Text(ref t) if t.contains("CloseStream")) {
                        return;
                    }
                }
                assert!(got_audio, "mock: expected to receive audio bytes");

                // Send a canned Results frame.
                let frame = r#"{
                  "type": "Results",
                  "channel": {"alternatives": [{"transcript": "the quick brown fox", "confidence": 0.97}]},
                  "is_final": true,
                  "start": 2.5,
                  "duration": 1.1
                }"#;
                let _ = write.send(Message::Text(frame.to_string().into())).await;

                // Keep the connection open briefly so the reader picks up the frame.
                tokio::time::sleep(Duration::from_millis(100)).await;
                let _ = write.close().await;
            });
        }
    });

    // 2. Wire up broadcast channels.
    let (audio_tx, audio_rx) = tokio::sync::broadcast::channel::<AudioChunk>(16);
    let (txn_tx, mut txn_rx) = tokio::sync::broadcast::channel::<TranscriptEvent>(16);

    // 3. Spawn the adapter, pointed at the mock.
    let mut stt = DeepgramStt::new("fake-key");
    stt.base_url = format!("ws://127.0.0.1:{port}");
    let stt = std::sync::Arc::new(stt);
    let stt2 = stt.clone();
    let adapter = tokio::spawn(async move {
        stt2.start(audio_rx, txn_tx).await.ok();
    });

    // 4. Push some audio.
    tokio::time::sleep(Duration::from_millis(150)).await; // let WS connections complete
    audio_tx
        .send(AudioChunk { channel: Channel::Mic, samples: vec![0i16; 320], ts_ms: 0 })
        .unwrap();
    audio_tx
        .send(AudioChunk { channel: Channel::System, samples: vec![0i16; 320], ts_ms: 0 })
        .unwrap();

    // 5. Assert we receive the mapped TranscriptEvent.
    let ev = tokio::time::timeout(Duration::from_secs(3), txn_rx.recv())
        .await
        .expect("transcript event within 3s")
        .expect("event received");

    assert_eq!(ev.text, "the quick brown fox");
    assert_eq!(ev.channel, Channel::Mic);
    assert!(ev.is_final);
    assert_eq!(ev.ts_ms, 2500);

    // 6. Cleanup.
    drop(audio_tx);
    let _ = tokio::time::timeout(Duration::from_secs(2), adapter).await;
    server.abort();
}
```

- [ ] **Step 2: Run.**

Run: `cargo test -p yogurt-stt --test deepgram_mock`
Expected: `1 passed` within ~2s.

If it hangs: most likely the adapter is opening sessions in series instead of parallel — check `spawn_session` is `.await`-ed twice without blocking between, and that the mock's `accept` loop accepts both connections.

- [ ] **Step 3: Commit.**

```bash
git add crates/yogurt-stt/tests/deepgram_mock.rs
git commit -m "test(stt): add mock-WS integration test for deepgram event mapping"
```

---

### Task 3.4 · `meetings.rs` — in-memory registry that wires audio → STT

**Files:**
- Modify: `crates/yogurt-server/Cargo.toml` — add `yogurt-stt`, `yogurt-audio`, `uuid`, `async-trait`.
- Create: `crates/yogurt-server/src/meetings.rs`
- Modify: `crates/yogurt-server/src/lib.rs` — register `Registry` as router state.

> **Important separation:** the `Registry` does NOT know about HTTP — it just owns `Arc<Meeting>` values and has methods like `create()`, `start(id)`, `stop(id)`, `subscribe(id)`. The HTTP/WS layer (Task 3.5 / 3.6) calls into it. This keeps `meetings.rs` unit-testable without spinning up axum, and makes Phase 7 (swap in SQLite) a same-file refactor.

- [ ] **Step 1: Update `crates/yogurt-server/Cargo.toml`.**

Append to `[dependencies]`:

```toml
yogurt-stt = { path = "../yogurt-stt" }
yogurt-audio = { path = "../yogurt-audio" }
uuid = { workspace = true }
async-trait = { workspace = true }
```

Append to `[dependencies]` (also needed — axum's WebSocket extractor is behind a feature flag):

```toml
axum = { workspace = true, features = ["macros", "ws"] }
```

(Phase 0 had `features = ["macros"]` only — replace that line with the above.)

Append to `[dev-dependencies]`:

```toml
tokio-tungstenite = { workspace = true }
futures-util = { workspace = true }
```

- [ ] **Step 2: Write `crates/yogurt-server/src/meetings.rs`.**

```rust
//! In-memory meeting registry. Phase 7 swaps this for SQLite persistence behind
//! the same public API (`Registry::create`, `start`, `stop`, `subscribe`).
//!
//! Each [`Meeting`] owns:
//!   - an audio broadcast (from `yogurt-audio`) that fans 16 kHz mono i16 PCM to
//!     all subscribers (currently just the STT engine, eventually also a level
//!     meter and waveform recorder),
//!   - a transcript broadcast (from `yogurt-stt`) that fans [`TranscriptEvent`]s to
//!     all WebSocket clients,
//!   - the JoinHandle of the audio + STT supervisor task, used to abort on stop.

use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use uuid::Uuid;
use yogurt_stt::{deepgram::DeepgramStt, AudioChunk, Stt, TranscriptEvent};

pub type MeetingId = Uuid;

/// One in-progress or just-ended meeting.
pub struct Meeting {
    pub id: MeetingId,
    pub created_at_ms: u64,
    /// Audio broadcast — populated by `yogurt-audio` when recording is live.
    /// Capacity 256 is ~5 seconds of 20ms chunks; lagged subscribers warn and drop frames.
    pub audio_tx: tokio::sync::broadcast::Sender<AudioChunk>,
    /// Transcript broadcast — populated by the STT engine, consumed by WS clients.
    /// Capacity 256 is plenty (transcripts arrive < 10 Hz).
    pub transcript_tx: tokio::sync::broadcast::Sender<TranscriptEvent>,
    /// `Some` while recording, `None` before start / after stop.
    pub task: tokio::sync::Mutex<Option<JoinHandle<()>>>,
}

impl Meeting {
    fn new() -> Self {
        let (audio_tx, _) = tokio::sync::broadcast::channel(256);
        let (transcript_tx, _) = tokio::sync::broadcast::channel(256);
        Self {
            id: Uuid::now_v7(),
            created_at_ms: now_ms(),
            audio_tx,
            transcript_tx,
            task: tokio::sync::Mutex::new(None),
        }
    }
}

#[derive(Default)]
pub struct Registry {
    meetings: RwLock<HashMap<MeetingId, Arc<Meeting>>>,
}

impl Registry {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub async fn create(&self) -> Arc<Meeting> {
        let m = Arc::new(Meeting::new());
        self.meetings.write().await.insert(m.id, m.clone());
        m
    }

    pub async fn get(&self, id: &MeetingId) -> Option<Arc<Meeting>> {
        self.meetings.read().await.get(id).cloned()
    }

    /// Start recording: spin up `yogurt-audio` capture and a Deepgram session.
    pub async fn start(&self, id: &MeetingId) -> Result<()> {
        let m = self.get(id).await.ok_or_else(|| anyhow!("meeting not found"))?;

        // Refuse to start twice.
        if m.task.lock().await.is_some() {
            return Err(anyhow!("meeting already started"));
        }

        let api_key = std::env::var("YOGURT_DEEPGRAM_API_KEY")
            .context("YOGURT_DEEPGRAM_API_KEY not set — required for cloud STT in Phase 3")?;

        let audio_tx = m.audio_tx.clone();
        let transcript_tx = m.transcript_tx.clone();
        let audio_rx_for_stt = m.audio_tx.subscribe();

        let task = tokio::spawn(async move {
            // Spawn the STT engine first (so it subscribes before audio starts flowing).
            let stt = Arc::new(DeepgramStt::new(api_key));
            let stt2 = stt.clone();
            let stt_handle = tokio::spawn(async move {
                if let Err(e) = stt2.start(audio_rx_for_stt, transcript_tx).await {
                    tracing::error!(?e, "stt session failed");
                }
            });

            // Then spawn audio capture, pushing into audio_tx.
            // yogurt-audio's API: AudioCapture::start() -> Stream<Item = AudioChunk>.
            if let Err(e) = yogurt_audio::capture_into(audio_tx).await {
                tracing::error!(?e, "audio capture failed");
            }

            stt_handle.abort();
        });

        *m.task.lock().await = Some(task);
        Ok(())
    }

    /// Stop recording. Idempotent — calling on an already-stopped meeting is a no-op.
    pub async fn stop(&self, id: &MeetingId) -> Result<()> {
        let m = self.get(id).await.ok_or_else(|| anyhow!("meeting not found"))?;
        if let Some(t) = m.task.lock().await.take() {
            t.abort();
        }
        Ok(())
    }

    /// Subscribe to the meeting's transcript broadcast.
    /// The receiver will see all events emitted from the subscribe call onward;
    /// older events are not replayed (matches PRD §5.2 — late joiners just see the
    /// live tail, not the meeting history).
    pub async fn subscribe(
        &self,
        id: &MeetingId,
    ) -> Option<tokio::sync::broadcast::Receiver<TranscriptEvent>> {
        Some(self.get(id).await?.transcript_tx.subscribe())
    }
}

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use yogurt_stt::Channel;

    #[tokio::test]
    async fn it_creates_meetings_with_unique_ids() {
        let reg = Registry::new();
        let m1 = reg.create().await;
        let m2 = reg.create().await;
        assert_ne!(m1.id, m2.id);
    }

    #[tokio::test]
    async fn it_fans_out_transcript_events_to_subscribers() {
        let reg = Registry::new();
        let m = reg.create().await;
        let mut rx1 = reg.subscribe(&m.id).await.unwrap();
        let mut rx2 = reg.subscribe(&m.id).await.unwrap();

        m.transcript_tx
            .send(TranscriptEvent {
                ts_ms: 100,
                channel: Channel::Mic,
                text: "hi".into(),
                is_final: false,
            })
            .unwrap();

        let a = rx1.recv().await.unwrap();
        let b = rx2.recv().await.unwrap();
        assert_eq!(a.text, "hi");
        assert_eq!(b.text, "hi");
    }
}
```

> **⚠ Note on `yogurt_audio::capture_into`:** Phase 2 must expose a function with signature `pub async fn capture_into(tx: broadcast::Sender<AudioChunk>) -> Result<()>` — i.e. it loops on real audio and pushes chunks until the sender drops or the underlying capture errors. If Phase 2 ended with a different shape (e.g. `start() -> impl Stream`), wrap it in this signature in `yogurt-audio`'s `lib.rs` before this task — do not bend `meetings.rs` around it. Re-using a stable trait-shaped seam here makes the test-with-synthetic-audio path in Task 3.10 work without touching production code.

- [ ] **Step 3: Wire `Registry` into `crates/yogurt-server/src/lib.rs`.**

Modify the existing `lib.rs` to:

```rust
mod assets;
mod dev_proxy;
pub mod meetings;
mod routes;
pub mod ws;

use anyhow::Result;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;

#[derive(Debug, Clone, Copy)]
pub enum Mode {
    Dev,
    Release,
}

/// Router state — shared `Arc<Registry>` for all handlers.
#[derive(Clone)]
pub struct AppState {
    pub meetings: Arc<meetings::Registry>,
}

pub async fn run(addr: SocketAddr, mode: Mode) -> Result<()> {
    let state = AppState { meetings: meetings::Registry::new() };
    let app = routes::router(mode, state);
    tracing::info!(?addr, ?mode, "yogurt-server starting");
    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
```

(Note: `pub mod meetings` and `pub mod ws` so tests can call them; Task 3.6 fills in `ws.rs`.)

- [ ] **Step 4: Add a temporary `ws.rs` stub so `lib.rs` compiles.**

Create `crates/yogurt-server/src/ws.rs`:

```rust
//! Real impl lands in Task 3.6.
```

- [ ] **Step 5: Verify everything still compiles.**

Run: `cargo check -p yogurt-server`
Expected: clean.

Run: `cargo test -p yogurt-server --lib meetings::tests`
Expected: `2 passed`.

- [ ] **Step 6: Commit.**

```bash
git add crates/yogurt-server/
git commit -m "feat(server): in-memory meeting registry that wires yogurt-audio + yogurt-stt"
```

---

### Task 3.5 · REST endpoints `POST /api/meetings` + `/start` + `/stop`

**Files:**
- Modify: `crates/yogurt-server/src/routes.rs`

- [ ] **Step 1: Update `crates/yogurt-server/src/routes.rs` to add the three endpoints and accept `AppState`.**

```rust
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::assets::serve_embedded;
use crate::ws::ws_meeting_handler;
use crate::{AppState, Mode};

pub fn router(mode: Mode, state: AppState) -> Router {
    let api = Router::new()
        .route("/api/health", get(health))
        .route("/api/meetings", post(create_meeting))
        .route("/api/meetings/:id/start", post(start_meeting))
        .route("/api/meetings/:id/stop", post(stop_meeting))
        .route("/ws/meetings/:id", get(ws_meeting_handler));

    let mut router = api.with_state(state);

    router = match mode {
        Mode::Release => router.fallback(serve_embedded),
        Mode::Dev => router.fallback(crate::dev_proxy::proxy_to_vite),
    };

    router
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok", "service": "yogurt-server" }))
}

async fn create_meeting(State(state): State<AppState>) -> Json<Value> {
    let m = state.meetings.create().await;
    Json(json!({ "id": m.id, "created_at_ms": m.created_at_ms }))
}

async fn start_meeting(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    match state.meetings.start(&id).await {
        Ok(_) => (StatusCode::OK, Json(json!({ "status": "started" }))).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn stop_meeting(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    match state.meetings.stop(&id).await {
        Ok(_) => (StatusCode::OK, Json(json!({ "status": "stopped" }))).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}
```

- [ ] **Step 2: Update the existing health test in `crates/yogurt-server/tests/health.rs`.**

The Phase 0 test still passes — it calls `yogurt_server::run` and hits `/api/health`. Re-run it to confirm refactor didn't break anything.

Run: `cargo test -p yogurt-server --test health`
Expected: `it_responds_to_health ... ok`.

- [ ] **Step 3: Add a new integration test for the REST lifecycle.**

Create `crates/yogurt-server/tests/meeting_rest.rs`:

```rust
use std::time::Duration;

#[tokio::test(flavor = "multi_thread")]
async fn it_creates_a_meeting_and_returns_an_id() {
    let addr: std::net::SocketAddr = "127.0.0.1:17890".parse().unwrap();
    let handle = tokio::spawn(async move {
        yogurt_server::run(addr, yogurt_server::Mode::Release).await
    });
    tokio::time::sleep(Duration::from_millis(200)).await;

    let body = reqwest::Client::new()
        .post("http://127.0.0.1:17890/api/meetings")
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();

    let id = body["id"].as_str().expect("id is a string");
    assert!(uuid::Uuid::parse_str(id).is_ok(), "id parses as uuid");
    assert!(body["created_at_ms"].as_u64().unwrap() > 0);

    handle.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn it_rejects_start_without_api_key() {
    // Ensure the env var is NOT set for this test.
    // SAFETY: tests in the same binary share the process env; we set unsafe-style
    // for clarity. If parallel tests need this var, gate the test on a serial mutex.
    std::env::remove_var("YOGURT_DEEPGRAM_API_KEY");

    let addr: std::net::SocketAddr = "127.0.0.1:17891".parse().unwrap();
    let handle = tokio::spawn(async move {
        yogurt_server::run(addr, yogurt_server::Mode::Release).await
    });
    tokio::time::sleep(Duration::from_millis(200)).await;

    let client = reqwest::Client::new();
    let created = client
        .post("http://127.0.0.1:17891/api/meetings")
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    let id = created["id"].as_str().unwrap();

    let resp = client
        .post(format!("http://127.0.0.1:17891/api/meetings/{id}/start"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let body = resp.json::<serde_json::Value>().await.unwrap();
    assert!(body["error"].as_str().unwrap().contains("YOGURT_DEEPGRAM_API_KEY"));

    handle.abort();
}
```

- [ ] **Step 4: Run.**

Run: `cargo test -p yogurt-server --test meeting_rest`
Expected: `2 passed`.

- [ ] **Step 5: Commit.**

```bash
git add crates/yogurt-server/
git commit -m "feat(server): POST /api/meetings + /start + /stop endpoints"
```

---

### Task 3.6 · WebSocket route `/ws/meetings/:id`

**Files:**
- Modify: `crates/yogurt-server/src/ws.rs` (replace stub with real impl)

> **What this handler does:** upgrade the HTTP request, look up the meeting in `Registry`, subscribe to its transcript broadcast, then loop: for each `TranscriptEvent`, serialize to JSON and push as a `Message::Text` frame. If the client disconnects, exit the loop. If the meeting doesn't exist, close with code 4404.

- [ ] **Step 1: Replace `crates/yogurt-server/src/ws.rs`.**

```rust
//! WebSocket handler for `/ws/meetings/:id`.
//!
//! S→C JSON frames per PRD §10: `transcript { ts_ms, channel, text, is_final }`.
//! C→S frames in v1: none yet (Phase 4 adds `notes_edit`, Phase 6 adds `chat_send`).
//! Client-sent frames are read-and-discarded so the WS stays bidirectionally healthy.

use axum::{
    extract::{
        ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    response::IntoResponse,
};
use uuid::Uuid;

use crate::AppState;

pub async fn ws_meeting_handler(
    ws: WebSocketUpgrade,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, id, state))
}

async fn handle_socket(mut socket: WebSocket, id: Uuid, state: AppState) {
    let mut rx = match state.meetings.subscribe(&id).await {
        Some(r) => r,
        None => {
            let _ = socket
                .send(Message::Close(Some(CloseFrame {
                    code: 4404,
                    reason: "meeting not found".into(),
                })))
                .await;
            return;
        }
    };

    loop {
        tokio::select! {
            // Server → Client: transcript events.
            ev = rx.recv() => match ev {
                Ok(ev) => {
                    let frame = match serde_json::to_string(&serde_json::json!({
                        "type": "transcript",
                        "payload": ev,
                    })) {
                        Ok(s) => s,
                        Err(e) => {
                            tracing::warn!(?e, "ws: serialize failed");
                            continue;
                        }
                    };
                    if socket.send(Message::Text(frame.into())).await.is_err() {
                        tracing::info!(meeting=%id, "ws: client disconnected");
                        return;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(meeting=%id, n, "ws: client lagged");
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    tracing::info!(meeting=%id, "ws: transcript stream ended");
                    return;
                }
            },
            // Client → Server: drained so the WS stays healthy.
            msg = socket.recv() => match msg {
                Some(Ok(Message::Close(_))) | None => {
                    tracing::info!(meeting=%id, "ws: client closed");
                    return;
                }
                Some(Ok(_)) => {
                    // ignore for now — Phase 4 (notes edit) and Phase 6 (chat) will route here
                }
                Some(Err(e)) => {
                    tracing::warn!(meeting=%id, ?e, "ws: client recv error");
                    return;
                }
            },
        }
    }
}
```

- [ ] **Step 2: Write the WS integration test.**

Create `crates/yogurt-server/tests/meeting_ws.rs`:

```rust
//! End-to-end WS test that doesn't depend on Deepgram: directly publish into the
//! meeting's transcript broadcast and assert it lands on the WS client.

use futures_util::StreamExt;
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;
use yogurt_server::AppState;

#[tokio::test(flavor = "multi_thread")]
async fn it_fans_transcript_events_to_ws_clients() {
    // Reach into the server to grab a handle on the Registry. We do that by
    // constructing the state ourselves and calling axum::serve with the same
    // router, instead of going through `yogurt_server::run`.
    let state = AppState { meetings: yogurt_server::meetings::Registry::new() };
    let app = yogurt_server::__test_router(state.clone());
    let addr: std::net::SocketAddr = "127.0.0.1:17892".parse().unwrap();
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });

    // Create a meeting via the registry directly (skip REST for this test).
    let m = state.meetings.create().await;

    // Connect a WS client.
    let url = format!("ws://127.0.0.1:17892/ws/meetings/{}", m.id);
    let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.unwrap();

    // Wait briefly for the handler to subscribe before publishing.
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Publish a transcript event.
    m.transcript_tx
        .send(yogurt_stt::TranscriptEvent {
            ts_ms: 11_020,
            channel: yogurt_stt::Channel::Mic,
            text: "hello from the test".into(),
            is_final: true,
        })
        .unwrap();

    // Read the frame.
    let msg = tokio::time::timeout(Duration::from_secs(2), ws.next())
        .await
        .expect("frame within 2s")
        .unwrap()
        .unwrap();
    let text = match msg {
        Message::Text(t) => t,
        other => panic!("expected text frame, got {other:?}"),
    };
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["type"], "transcript");
    assert_eq!(v["payload"]["text"], "hello from the test");
    assert_eq!(v["payload"]["channel"], "mic");
    assert_eq!(v["payload"]["ts_ms"], 11_020);

    server.abort();
}
```

- [ ] **Step 3: Expose `__test_router` for the test.**

Append to `crates/yogurt-server/src/lib.rs` (just above the `run` function):

```rust
/// Construct a router with a custom AppState, for tests that want to reach into
/// the registry directly instead of going through the REST surface.
///
/// Not part of the stable public API — leading underscore communicates that.
#[doc(hidden)]
pub fn __test_router(state: AppState) -> axum::Router {
    routes::router(Mode::Release, state)
}
```

- [ ] **Step 4: Run.**

Run: `cargo test -p yogurt-server --test meeting_ws`
Expected: `1 passed`.

- [ ] **Step 5: Commit.**

```bash
git add crates/yogurt-server/
git commit -m "feat(server): GET /ws/meetings/:id pushes transcript events as JSON"
```

---

### Task 3.7 · Frontend: WebSocket client hook + animation tokens

**Files:**
- Modify: `web/src/index.css` — add `@keyframes slideInRight` + dock animation utilities.
- Create: `web/src/lib/ws.ts` — `useTranscriptWs` hook.

- [ ] **Step 1: Append animation tokens to `web/src/index.css`.**

The Phase 1 plan will move these into a proper tokens file; for Phase 3 they live in `index.css` to keep scope tight.

```css
/* === Phase 3: live transcript dock motion (PRD §16.5) === */

@keyframes slideInRight {
  from { transform: translateX(100%); }
  to   { transform: translateX(0); }
}

@keyframes slideOutRight {
  from { transform: translateX(0); }
  to   { transform: translateX(100%); }
}

.dock-open {
  animation: slideInRight 340ms cubic-bezier(.2, .7, .2, 1) both;
}

.dock-closed {
  animation: slideOutRight 340ms cubic-bezier(.2, .7, .2, 1) both;
}
```

- [ ] **Step 2: Write `web/src/lib/ws.ts`.**

```ts
import { useEffect, useRef, useState } from "react";

export type Channel = "mic" | "system";

export interface TranscriptEvent {
  ts_ms: number;
  channel: Channel;
  text: string;
  is_final: boolean;
}

interface WsFrame {
  type: "transcript";
  payload: TranscriptEvent;
}

/**
 * Subscribe to `/ws/meetings/:id`. Accumulates events into an in-memory array;
 * the latest partial per channel is held separately and replaced on each new
 * non-final event for that channel.
 *
 * Phase 3 doesn't persist anything — the array resets when the component unmounts.
 */
export function useTranscriptWs(meetingId: string | null): {
  events: TranscriptEvent[];
  connected: boolean;
} {
  const [events, setEvents] = useState<TranscriptEvent[]>([]);
  const [connected, setConnected] = useState(false);
  const wsRef = useRef<WebSocket | null>(null);

  useEffect(() => {
    if (!meetingId) return;
    const proto = window.location.protocol === "https:" ? "wss:" : "ws:";
    const url = `${proto}//${window.location.host}/ws/meetings/${meetingId}`;
    const ws = new WebSocket(url);
    wsRef.current = ws;

    ws.onopen = () => setConnected(true);
    ws.onclose = () => setConnected(false);
    ws.onerror = (e) => console.warn("transcript ws error:", e);
    ws.onmessage = (e) => {
      try {
        const frame = JSON.parse(e.data as string) as WsFrame;
        if (frame.type !== "transcript") return;
        setEvents((prev) => mergeEvent(prev, frame.payload));
      } catch (err) {
        console.warn("transcript ws: bad json", err);
      }
    };

    return () => {
      ws.close();
    };
  }, [meetingId]);

  return { events, connected };
}

/**
 * Merge a new event into the list. Strategy:
 *   - If the new event is `is_final`, push it.
 *   - If the new event is partial AND the last event on the same channel is also
 *     partial, replace it. Otherwise push.
 * Result: a stable list of finals with at most one trailing partial per channel.
 */
function mergeEvent(prev: TranscriptEvent[], ev: TranscriptEvent): TranscriptEvent[] {
  if (ev.is_final) return [...prev, ev];
  const lastIdx = findLastIndex(prev, (x) => x.channel === ev.channel);
  if (lastIdx === -1) return [...prev, ev];
  const last = prev[lastIdx];
  if (last.is_final) return [...prev, ev];
  const next = prev.slice();
  next[lastIdx] = ev;
  return next;
}

function findLastIndex<T>(arr: T[], pred: (x: T) => boolean): number {
  for (let i = arr.length - 1; i >= 0; i--) if (pred(arr[i])) return i;
  return -1;
}
```

- [ ] **Step 3: Vitest for the hook.**

Create `web/src/lib/ws.test.ts`:

```ts
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useTranscriptWs } from "./ws";

class MockWebSocket {
  onopen: ((this: WebSocket, ev: Event) => unknown) | null = null;
  onclose: ((this: WebSocket, ev: CloseEvent) => unknown) | null = null;
  onerror: ((this: WebSocket, ev: Event) => unknown) | null = null;
  onmessage: ((this: WebSocket, ev: MessageEvent) => unknown) | null = null;
  readyState = 0;
  close = vi.fn();
  constructor(public url: string) {
    queueMicrotask(() => {
      this.readyState = 1;
      this.onopen?.call(this as unknown as WebSocket, new Event("open"));
    });
  }
  emit(data: string) {
    this.onmessage?.call(this as unknown as WebSocket, new MessageEvent("message", { data }));
  }
}

let lastWs: MockWebSocket | null = null;

beforeEach(() => {
  lastWs = null;
  vi.stubGlobal("WebSocket", class extends MockWebSocket {
    constructor(url: string) {
      super(url);
      lastWs = this;
    }
  });
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("useTranscriptWs", () => {
  it("merges a partial then a final into the events list", async () => {
    const { result } = renderHook(() => useTranscriptWs("test-id"));
    await act(async () => {});

    act(() => {
      lastWs!.emit(JSON.stringify({
        type: "transcript",
        payload: { ts_ms: 1000, channel: "mic", text: "hel", is_final: false },
      }));
    });
    expect(result.current.events).toHaveLength(1);
    expect(result.current.events[0].text).toBe("hel");

    act(() => {
      lastWs!.emit(JSON.stringify({
        type: "transcript",
        payload: { ts_ms: 1000, channel: "mic", text: "hello", is_final: false },
      }));
    });
    expect(result.current.events).toHaveLength(1);
    expect(result.current.events[0].text).toBe("hello");

    act(() => {
      lastWs!.emit(JSON.stringify({
        type: "transcript",
        payload: { ts_ms: 1100, channel: "mic", text: "hello world", is_final: true },
      }));
    });
    expect(result.current.events).toHaveLength(1);
    expect(result.current.events[0].is_final).toBe(true);
    expect(result.current.events[0].text).toBe("hello world");
  });

  it("keeps mic and system partials independent", async () => {
    const { result } = renderHook(() => useTranscriptWs("test-id"));
    await act(async () => {});

    act(() => {
      lastWs!.emit(JSON.stringify({
        type: "transcript",
        payload: { ts_ms: 100, channel: "mic", text: "me talking", is_final: false },
      }));
      lastWs!.emit(JSON.stringify({
        type: "transcript",
        payload: { ts_ms: 100, channel: "system", text: "them talking", is_final: false },
      }));
    });

    expect(result.current.events).toHaveLength(2);
    expect(result.current.events[0].channel).toBe("mic");
    expect(result.current.events[1].channel).toBe("system");
  });
});
```

- [ ] **Step 4: Run.**

Run: `pnpm --dir web test`
Expected: previous Phase 0 tests still pass + 2 new tests in `ws.test.ts` pass.

- [ ] **Step 5: Commit.**

```bash
git add web/src/index.css web/src/lib/ws.ts web/src/lib/ws.test.ts
git commit -m "feat(web): useTranscriptWs hook + slideInRight dock keyframes"
```

---

### Task 3.8 · `TranscriptLine` + `TranscriptDock` components

**Files:**
- Create: `web/src/components/TranscriptLine.tsx`
- Create: `web/src/components/TranscriptDock.tsx`
- Create: `web/src/components/TranscriptDock.test.tsx`

- [ ] **Step 1: Write `web/src/components/TranscriptLine.tsx`.**

```tsx
import type { TranscriptEvent } from "../lib/ws";

/**
 * One transcript line.
 *   "Me"  (mic)    = ink black label (#211D18)
 *   "Them" (system) = grey label (#A89F90)
 * Timestamp in JetBrains Mono, formatted as HH:MM:SS from meeting start.
 *
 * Phase 1 will replace the inline hex with token classes (text-ink, text-grey,
 * font-mono). For Phase 3 we keep tokens inline so this component renders
 * correctly even before the design-system phase lands.
 */
export function TranscriptLine({ ev }: { ev: TranscriptEvent }) {
  const isMe = ev.channel === "mic";
  return (
    <div
      className="py-2 text-[14px] leading-snug"
      data-channel={ev.channel}
      data-final={ev.is_final}
    >
      <span
        className="mr-2 inline-block w-10 font-semibold"
        style={{ color: isMe ? "#211D18" : "#A89F90" }}
      >
        {isMe ? "Me" : "Them"}
      </span>
      <span
        className="mr-2 text-[12px]"
        style={{ fontFamily: "JetBrains Mono, ui-monospace, monospace", color: "#A89F90" }}
      >
        {formatTs(ev.ts_ms)}
      </span>
      <span style={{ color: isMe ? "#211D18" : "#A89F90", opacity: ev.is_final ? 1 : 0.7 }}>
        {ev.text}
      </span>
    </div>
  );
}

function formatTs(ms: number): string {
  const total = Math.floor(ms / 1000);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  return `${String(h).padStart(2, "0")}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
}
```

- [ ] **Step 2: Write `web/src/components/TranscriptDock.tsx`.**

```tsx
import { useEffect, useRef, useState } from "react";
import { useTranscriptWs } from "../lib/ws";
import { TranscriptLine } from "./TranscriptLine";

/**
 * Right-edge collapsible dock per PRD §5.2.
 *   - Collapsed by default: a vertical tab pinned to the right edge with the
 *     label "Live transcript" + a 3-bar wave glyph.
 *   - Click → expand: 330px-wide panel slides in from the right (340ms
 *     cubic-bezier(.2,.7,.2,1) via the `.dock-open` keyframe in index.css).
 *   - The dock is `position: fixed` and the parent layout reserves the right
 *     gutter — notes stay fully editable behind it (no z-index war).
 *   - Auto-scrolls to bottom when new events arrive UNLESS the user scrolled up.
 */
export function TranscriptDock({ meetingId }: { meetingId: string | null }) {
  const [open, setOpen] = useState(false);
  const { events, connected } = useTranscriptWs(meetingId);

  const listRef = useRef<HTMLDivElement | null>(null);
  const stickyRef = useRef(true);

  // Auto-scroll to bottom on new events, unless user scrolled up.
  useEffect(() => {
    const list = listRef.current;
    if (!list || !stickyRef.current) return;
    list.scrollTop = list.scrollHeight;
  }, [events]);

  function onScroll() {
    const list = listRef.current;
    if (!list) return;
    const atBottom = list.scrollHeight - list.scrollTop - list.clientHeight < 24;
    stickyRef.current = atBottom;
  }

  return (
    <div className="fixed right-0 top-0 h-full z-30 pointer-events-none">
      <div className="relative h-full flex pointer-events-auto">
        {/* The collapsed tab is always rendered so the user can re-collapse. */}
        <button
          type="button"
          aria-label={open ? "Hide live transcript" : "Show live transcript"}
          onClick={() => setOpen((v) => !v)}
          className="self-center -mr-px h-24 w-7 rounded-l-md border border-r-0 bg-white text-[11px] flex items-center justify-center"
          style={{ borderColor: "#EBE3D5", writingMode: "vertical-rl" }}
        >
          <span className="mr-1">{open ? "▶" : "◀"}</span>
          Live transcript
        </button>

        {/* The sliding panel is mounted/unmounted on open so the keyframe re-runs. */}
        {open && (
          <aside
            data-testid="transcript-dock-panel"
            className="dock-open w-[330px] h-full bg-white border-l flex flex-col"
            style={{ borderColor: "#EBE3D5" }}
          >
            <header className="px-4 py-3 border-b flex items-center justify-between"
                    style={{ borderColor: "#EBE3D5" }}>
              <div className="text-[13px] font-semibold" style={{ color: "#211D18" }}>
                Live transcript
              </div>
              <div className="text-[11px]" style={{ color: connected ? "#5E9E73" : "#A89F90" }}>
                {connected ? "● connected" : "○ offline"}
              </div>
            </header>
            <div
              ref={listRef}
              onScroll={onScroll}
              className="flex-1 overflow-y-auto px-4 py-2"
              data-testid="transcript-list"
            >
              {events.length === 0 ? (
                <div className="text-[12px]" style={{ color: "#A89F90" }}>
                  Waiting for audio…
                </div>
              ) : (
                events.map((ev, i) => <TranscriptLine key={i} ev={ev} />)
              )}
            </div>
          </aside>
        )}
      </div>
    </div>
  );
}
```

- [ ] **Step 3: Write the Vitest for the dock.**

Create `web/src/components/TranscriptDock.test.tsx`:

```tsx
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, act } from "@testing-library/react";
import { TranscriptDock } from "./TranscriptDock";

class MockWebSocket {
  onopen: ((ev: Event) => unknown) | null = null;
  onclose: ((ev: CloseEvent) => unknown) | null = null;
  onerror: ((ev: Event) => unknown) | null = null;
  onmessage: ((ev: MessageEvent) => unknown) | null = null;
  close = vi.fn();
  constructor(public url: string) {
    lastWs = this;
    queueMicrotask(() => this.onopen?.(new Event("open")));
  }
  emit(data: string) {
    this.onmessage?.(new MessageEvent("message", { data }));
  }
}
let lastWs: MockWebSocket | null = null;

beforeEach(() => {
  lastWs = null;
  vi.stubGlobal("WebSocket", MockWebSocket);
});
afterEach(() => vi.unstubAllGlobals());

describe("TranscriptDock", () => {
  it("renders the collapsed tab by default (panel hidden)", () => {
    render(<TranscriptDock meetingId="abc" />);
    expect(screen.getByLabelText(/Show live transcript/i)).toBeInTheDocument();
    expect(screen.queryByTestId("transcript-dock-panel")).toBeNull();
  });

  it("expands the panel on click and applies dock-open animation class", async () => {
    render(<TranscriptDock meetingId="abc" />);
    fireEvent.click(screen.getByLabelText(/Show live transcript/i));
    const panel = screen.getByTestId("transcript-dock-panel");
    expect(panel.className).toMatch(/dock-open/);
  });

  it("renders Me/Them labels with the right colors when events arrive", async () => {
    render(<TranscriptDock meetingId="abc" />);
    fireEvent.click(screen.getByLabelText(/Show live transcript/i));
    await act(async () => {});

    act(() => {
      lastWs!.emit(JSON.stringify({
        type: "transcript",
        payload: { ts_ms: 0, channel: "mic", text: "hi there", is_final: true },
      }));
      lastWs!.emit(JSON.stringify({
        type: "transcript",
        payload: { ts_ms: 500, channel: "system", text: "hello back", is_final: true },
      }));
    });

    const list = screen.getByTestId("transcript-list");
    const lines = list.querySelectorAll("[data-channel]");
    expect(lines).toHaveLength(2);

    const me = lines[0] as HTMLElement;
    const them = lines[1] as HTMLElement;
    expect(me.querySelector("span")?.textContent).toBe("Me");
    expect(them.querySelector("span")?.textContent).toBe("Them");
    expect((me.querySelector("span") as HTMLElement).style.color).toBe("rgb(33, 29, 24)");
    expect((them.querySelector("span") as HTMLElement).style.color).toBe("rgb(168, 159, 144)");
  });
});
```

- [ ] **Step 4: Run.**

Run: `pnpm --dir web test`
Expected: all previous tests + 3 new in `TranscriptDock.test.tsx` pass.

- [ ] **Step 5: Commit.**

```bash
git add web/src/components/
git commit -m "feat(web): collapsible TranscriptDock with Me/Them lines + 340ms slide-in"
```

---

### Task 3.9 · `Meeting.tsx` route + minimal routing in `App.tsx`

**Files:**
- Create: `web/src/routes/Meeting.tsx`
- Modify: `web/src/App.tsx` — swap the Phase 0 hello-page for a tiny route enum that launches a meeting.

- [ ] **Step 1: Write `web/src/routes/Meeting.tsx`.**

```tsx
import { useEditor, EditorContent } from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import { useState } from "react";
import { TranscriptDock } from "../components/TranscriptDock";

interface CreateMeetingResp { id: string; created_at_ms: number; }

/**
 * Phase 3 meeting view: notes column (TipTap from Phase 0) + transcript dock.
 * NO persistence — the notes never leave the page. NO `aiGrey` mark — Phase 4.
 *
 * The notes column gets a right padding equal to the dock's tab width (28px) so
 * the cursor never sits behind the tab. When the dock expands to 330px, the
 * notes stay at max-width 660px (PRD §16.8) and just lose some right whitespace
 * — they DO NOT reflow. This matches PRD §5.2: "Notes stay fully editable while
 * the panel is open."
 */
export function Meeting() {
  const [meetingId, setMeetingId] = useState<string | null>(null);
  const [recording, setRecording] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const editor = useEditor({
    extensions: [StarterKit],
    content: "<p>Take sparse notes during the meeting — AI enhances on End (Phase 4).</p>",
  });

  async function createMeeting() {
    setError(null);
    const r = await fetch("/api/meetings", { method: "POST" });
    const j: CreateMeetingResp = await r.json();
    setMeetingId(j.id);
  }

  async function startRecording() {
    if (!meetingId) return;
    setError(null);
    const r = await fetch(`/api/meetings/${meetingId}/start`, { method: "POST" });
    if (!r.ok) {
      const e = await r.json();
      setError(e.error ?? `start failed: ${r.status}`);
      return;
    }
    setRecording(true);
  }

  async function stopRecording() {
    if (!meetingId) return;
    await fetch(`/api/meetings/${meetingId}/stop`, { method: "POST" });
    setRecording(false);
  }

  return (
    <div className="min-h-screen pr-7">
      <main className="max-w-[660px] mx-auto px-6 pt-12 pb-32">
        <header className="mb-8 flex items-center justify-between">
          <h1 className="text-2xl font-semibold" style={{ color: "#211D18" }}>
            {meetingId ? `Meeting · ${meetingId.slice(0, 8)}` : "New meeting"}
          </h1>
          <div className="flex gap-2 text-[13px]">
            {!meetingId && (
              <button onClick={createMeeting}
                      className="px-3 py-1.5 rounded-md text-white"
                      style={{ backgroundColor: "#5B4FC7" }}>
                Create
              </button>
            )}
            {meetingId && !recording && (
              <button onClick={startRecording}
                      className="px-3 py-1.5 rounded-md text-white"
                      style={{ backgroundColor: "#5B4FC7" }}>
                Start recording
              </button>
            )}
            {meetingId && recording && (
              <button onClick={stopRecording}
                      className="px-3 py-1.5 rounded-md border"
                      style={{ borderColor: "#EBE3D5", color: "#211D18" }}>
                Stop
              </button>
            )}
          </div>
        </header>

        {error && (
          <div className="mb-4 p-3 rounded border text-[13px]"
               style={{ borderColor: "#E07A66", color: "#211D18", background: "#FCEFEB" }}>
            {error}
          </div>
        )}

        <section className="rounded-lg p-4 border bg-white" style={{ borderColor: "#EBE3D5" }}>
          <EditorContent editor={editor} />
        </section>
      </main>

      <TranscriptDock meetingId={meetingId} />
    </div>
  );
}
```

- [ ] **Step 2: Update `web/src/App.tsx`.**

Replace the Phase 0 hello-world with a tiny route switch:

```tsx
import { useState } from "react";
import { Meeting } from "./routes/Meeting";

type View = "library" | "meeting";

export function App() {
  const [view, setView] = useState<View>("library");

  if (view === "meeting") return <Meeting />;

  // Stub library — real library lands in Phase 7.
  return (
    <main className="max-w-2xl mx-auto p-10 space-y-6">
      <h1 className="text-3xl font-bold tracking-tight">yogurt</h1>
      <p className="text-sm text-neutral-500">Phase 3 · the meeting library lands in Phase 7.</p>
      <button onClick={() => setView("meeting")}
              className="px-4 py-2 rounded-md text-white"
              style={{ backgroundColor: "#5B4FC7" }}>
        Open a new meeting →
      </button>
    </main>
  );
}
```

- [ ] **Step 3: Update the Phase 0 `App.test.tsx`.**

The Phase 0 test asserted on the health-fetch line in `App.tsx` — that line is gone now. Replace `web/src/App.test.tsx` with:

```tsx
import { describe, it, expect } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { App } from "./App";

describe("App", () => {
  it("renders the library stub by default", () => {
    render(<App />);
    expect(screen.getByRole("heading", { name: /yogurt/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Open a new meeting/i })).toBeInTheDocument();
  });

  it("switches to the meeting view on click", () => {
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: /Open a new meeting/i }));
    expect(screen.getByRole("heading", { name: /New meeting/i })).toBeInTheDocument();
  });
});
```

- [ ] **Step 4: Run.**

Run: `pnpm --dir web test`
Expected: all tests pass.

Run: `pnpm --dir web build`
Expected: tsc + vite both succeed.

- [ ] **Step 5: Commit.**

```bash
git add web/src/App.tsx web/src/App.test.tsx web/src/routes/
git commit -m "feat(web): Meeting route with notes column + dock + Start/Stop controls"
```

---

### Task 3.10 · End-to-end: synthetic-audio integration test + manual smoke + acceptance

**Files:**
- Create: `crates/yogurt-server/tests/e2e_synthetic_audio.rs`

> **Strategy:** This test bypasses Deepgram (we'd need a real API key in CI) and instead reaches into the meeting's `transcript_tx` directly to simulate an STT engine. It exercises the full path *from* publishing a transcript event *to* the WS client receiving it under wall-clock budget — proving the **< 2s lag** acceptance criterion on the server side. (Manual smoke in Step 3 covers the real Deepgram path.)

- [ ] **Step 1: Write the E2E test.**

```rust
//! Acceptance test for PRD §5.2: "transcript appears with < 2s lag using Deepgram".
//!
//! We test the SERVER side of that budget: from `transcript_tx.send(...)` to the
//! WS client receiving the frame, the round-trip must be well under 2s. Network
//! lag to Deepgram and Deepgram's own processing are separately budgeted; this
//! test pins the part we own.

use futures_util::StreamExt;
use std::time::{Duration, Instant};
use tokio_tungstenite::tungstenite::Message;
use yogurt_server::AppState;
use yogurt_stt::{Channel, TranscriptEvent};

#[tokio::test(flavor = "multi_thread")]
async fn it_delivers_transcript_to_browser_well_under_2s() {
    let state = AppState { meetings: yogurt_server::meetings::Registry::new() };
    let app = yogurt_server::__test_router(state.clone());
    let addr: std::net::SocketAddr = "127.0.0.1:17893".parse().unwrap();
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.ok() });

    let m = state.meetings.create().await;
    let url = format!("ws://127.0.0.1:17893/ws/meetings/{}", m.id);
    let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    let t0 = Instant::now();
    m.transcript_tx
        .send(TranscriptEvent {
            ts_ms: 0,
            channel: Channel::Mic,
            text: "fast path".into(),
            is_final: true,
        })
        .unwrap();

    let msg = tokio::time::timeout(Duration::from_secs(2), ws.next())
        .await
        .expect("must arrive within 2s")
        .unwrap()
        .unwrap();
    let elapsed = t0.elapsed();
    match msg {
        Message::Text(t) => assert!(t.contains("fast path")),
        other => panic!("expected text: {other:?}"),
    }

    // The actual budget we own (sender → ws frame) should be tens of milliseconds.
    // We assert 200ms here as a generous CI-friendly ceiling — anything close to
    // 2s would point at a real bug.
    assert!(
        elapsed < Duration::from_millis(200),
        "server-side lag was {elapsed:?}, expected < 200ms"
    );

    server.abort();
}
```

- [ ] **Step 2: Run.**

Run: `cargo test -p yogurt-server --test e2e_synthetic_audio`
Expected: `1 passed` in < 1s.

- [ ] **Step 3: Manual three-terminal smoke — the real Deepgram path.**

This requires a real Deepgram API key. Skip if you don't have one — the synthetic test above and the mock-WS test in Task 3.3 already cover the code path.

Terminal 1: `pnpm --dir web dev`
Terminal 2: `cargo run -p yogurt -- start --dev --no-open`

The Deepgram key can come from either:
- `.env.local` in the repo root (gitignored; preferred for dev) containing `YOGURT_DEEPGRAM_API_KEY=<your key>`
- An inline export: `export YOGURT_DEEPGRAM_API_KEY=<your key> && cargo run ...`

(Phase 3 does not yet wire `dotenvy` for `.env.local` loading — that lands in Phase 5 with the full env-var bootstrap pattern. For now, either `export` the var or use a shell that auto-loads `.env.local` like `direnv`.)
Terminal 3 (browser): open `http://localhost:7878`, click "Open a new meeting →", click "Create", click "Start recording", grant Screen Recording permission if prompted, talk into your mic for ~10 seconds.

Expected:
- Within ~1-2s of speaking, a "Me" line appears in the dock with your words.
- If something else is playing through system audio (a YouTube video, a Zoom call), "Them" lines appear too.
- The dock tab is visible on the right edge before clicking; clicking opens it with a visible slide animation.
- Notes column stays fully editable — try typing in it while the dock is open. Cursor lands where you expect.

If something doesn't work:
- No transcript: check the Rust logs for `deepgram: connecting` and any error after that. If you see `Authorization failed`, the API key is wrong.
- Audio not flowing: check Phase 2 — `yogurt_audio::capture_into` must be pushing chunks. Add a `tracing::debug!` in the loop if needed.

- [ ] **Step 4: Verify the animation token is in compiled CSS.**

The acceptance constraint says the 340ms `cubic-bezier(.2,.7,.2,1)` must be inspectable in compiled CSS.

Run:
```bash
pnpm --dir web build
grep -E "slideInRight|cubic-bezier\(.2.*.7.*.2.*1\)|340ms" web/dist/assets/*.css
```

Expected: matches both the `@keyframes slideInRight` rule and the `animation: slideInRight 340ms cubic-bezier(.2, .7, .2, 1)` declaration in the bundled CSS.

- [ ] **Step 5: Format + lint.**

Run: `cargo fmt --all && cargo clippy --all-targets -- -D warnings`
Expected: clean.

Run: `pnpm --dir web build`
Expected: clean.

- [ ] **Step 6: Commit.**

```bash
git add crates/yogurt-server/tests/e2e_synthetic_audio.rs
git commit -m "test(server): end-to-end synthetic-audio test asserts < 200ms server-side lag"
```

- [ ] **Step 7: Push.**

```bash
git push origin main
```

- [ ] **Step 8: Tag the phase milestone — only with explicit user confirmation.**

Pushing a tag is a public, semi-permanent action. Confirm with the user before:

```bash
git tag -a v0.0.3-phase-3 -m "Phase 3 complete: cloud STT (Deepgram streaming) + transcript dock"
git push origin v0.0.3-phase-3
```

---

## Phase 3 acceptance criteria

All six must be true:

1. `cargo test --workspace` passes (Phase 0 + Phase 2 + new Phase 3 tests).
2. `pnpm --dir web test` passes (Phase 0 smoke + new `ws.test.ts` + new `TranscriptDock.test.tsx`).
3. **REST lifecycle works:** `curl -X POST localhost:7878/api/meetings` returns a UUID; `POST /api/meetings/<id>/start` returns 200 (with `YOGURT_DEEPGRAM_API_KEY` set) or 400 (without).
4. **WS streaming works:** opening `ws://localhost:7878/ws/meetings/<id>` and pushing a transcript event server-side results in a `{"type":"transcript","payload":{...}}` frame on the client within < 200ms.
5. **Dock animation correct:** the compiled CSS for the dock open animation contains `340ms` and `cubic-bezier(.2, .7, .2, 1)`. The `TranscriptDock` test asserts the `dock-open` class is applied on expand.
6. **End-to-end with real Deepgram (manual):** speaking into the mic produces a "Me" transcript line in the browser dock within 2 seconds (PRD §5.2 acceptance).

## What this phase does NOT do

Explicitly out of scope (next plans cover these):
- Persistence of meetings or transcripts to SQLite (Phase 7) — meetings vanish on server restart.
- Local `whisper.cpp` STT (Phase 8) — only the cloud Deepgram adapter ships here.
- Settings UI for managing the Deepgram API key (Phase 5) — env var only.
- The `aiGrey` / `transcriptTs` TipTap marks and `↳ HH:MM` deep links (Phase 4).
- The Ask-this-meeting floating pill (Phase 6).
- Brand tokens (`--ink`, `--grey`, etc.) in CSS variables — Phase 3 inlines hex; Phase 1 will move them into tokens. The `TranscriptLine` already uses the correct hex values from PRD §16.2 so the Phase 1 refactor is a sed-and-replace.
- Audio level metering on the dock tab (only the static "Live transcript" label + arrow icon ship here; the 3-bar animated wave glyph from PRD §5.2 lands in Phase 1 / Phase 7).

## Next plan

After Phase 3 lands, write `docs/superpowers/plans/<date>-yogurt-phase-4-augmented-notes.md` covering:
- TipTap custom marks `aiGrey` (`color: #A89F90`) and `transcriptTs` (data attribute + dotted-underline lilac).
- Markdown round-trip via `pulldown-cmark` (Rust side) + a serializer that preserves marks.
- `POST /api/meetings/:id/enhance` endpoint that streams enhanced markdown back via the existing WS (new `enhance_progress` + `enhanced_md` frames per PRD §10).
- The shimmer-skeleton / staggered-reveal post-enhance UI per PRD §5.3 (motion tokens from §16.5: 140 / 340 / 560 / 760ms cascade).
- The structural-diff promote-to-black logic when the user edits a grey range.

Subsequent phase plans follow the PRD §12 roadmap (5: LLM client + settings, 6: in-meeting chat, 7: library + onboarding, 8: local STT, 9: polish + distribution).
