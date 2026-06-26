//! In-memory meeting registry. Phase 7 swaps this for SQLite persistence behind
//! the same public API (`Registry::create`, `start`, `stop`, `subscribe`).
//!
//! Each [`Meeting`] owns:
//!   - an audio broadcast (mirroring `yogurt-audio`'s `Frame` stream into
//!     `yogurt-stt`'s `AudioChunk` shape) that the STT engine subscribes to,
//!   - a transcript broadcast (from `yogurt-stt`) that fans [`TranscriptEvent`]s
//!     to all WebSocket clients,
//!   - the JoinHandle of the audio + STT supervisor task, used to abort on stop.
//!
//! Phase 3 wiring strategy:
//!   1. `start()` reads `YOGURT_DEEPGRAM_API_KEY` (D-07; Phase 5 swaps to Keychain).
//!   2. Spawns a supervisor task that opens `yogurt_audio::start_capture()` and
//!      holds the resulting `AudioStream` for the lifetime of the meeting
//!      (RAII: dropping it stops both mic + system capture).
//!   3. A small adapter task subscribes to `AudioStream::subscribe_mic()` and
//!      `subscribe_system()`, converts each `Frame` into a `yogurt_stt::AudioChunk`
//!      (channel + samples + ts_ms = monotonic_micros / 1000) and republishes
//!      it on the meeting's `audio_tx`.
//!   4. `DeepgramStt::new(api_key).start(audio_rx, transcript_tx)` runs the
//!      cloud STT session, emitting `TranscriptEvent`s onto `transcript_tx`.
//!   5. WS clients call `Registry::subscribe(id)` to attach a fresh receiver
//!      onto the transcript broadcast.

use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, oneshot, Mutex, RwLock};
use tokio::task::JoinHandle;
use uuid::Uuid;
use yogurt_stt::{deepgram::DeepgramStt, AudioChunk, Channel, Stt, TranscriptEvent};

pub type MeetingId = Uuid;

/// One in-progress or just-ended meeting.
///
/// `audio_tx` is the bridge between `yogurt-audio` (which speaks `Frame`)
/// and `yogurt-stt` (which speaks `AudioChunk`). The supervisor task spawned
/// by `Registry::start` adapts Frame → AudioChunk and republishes on this
/// sender so the STT engine subscriber sees a unified stream tagged with
/// the correct `Channel` enum value.
pub struct Meeting {
    pub id: MeetingId,
    pub created_at_ms: u64,
    /// Audio broadcast — populated by the Frame-→-AudioChunk adapter task
    /// inside the supervisor while recording is live. Capacity 256 is ~5
    /// seconds of 20ms chunks; lagged subscribers warn and drop frames.
    pub audio_tx: broadcast::Sender<AudioChunk>,
    /// Transcript broadcast — populated by the STT engine, consumed by WS
    /// clients. Capacity 256 is plenty (transcripts arrive < 10 Hz).
    pub transcript_tx: broadcast::Sender<TranscriptEvent>,
    /// `Some` while recording, `None` before start / after stop.
    pub task: Mutex<Option<JoinHandle<()>>>,
}

impl Meeting {
    fn new() -> Self {
        let (audio_tx, _) = broadcast::channel(256);
        let (transcript_tx, _) = broadcast::channel(256);
        Self {
            id: Uuid::now_v7(),
            created_at_ms: now_ms(),
            audio_tx,
            transcript_tx,
            task: Mutex::new(None),
        }
    }
}

impl std::fmt::Debug for Meeting {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Meeting")
            .field("id", &self.id)
            .field("created_at_ms", &self.created_at_ms)
            .field("audio_receivers", &self.audio_tx.receiver_count())
            .field("transcript_receivers", &self.transcript_tx.receiver_count())
            .finish_non_exhaustive()
    }
}

/// In-memory registry of live meetings. Phase 7 swaps the `HashMap` for
/// SQLite-backed persistence behind the same `Registry::{create, start,
/// stop, subscribe}` API.
#[derive(Default, Debug)]
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

    /// Start recording: spin up `yogurt-audio` capture + a Deepgram session.
    ///
    /// Errors:
    ///   - meeting not found
    ///   - meeting already started
    ///   - `YOGURT_DEEPGRAM_API_KEY` missing (D-07)
    ///   - `yogurt_audio::start_capture()` failed (caller will surface as
    ///     a 400 to the user — typically a permission gate)
    pub async fn start(&self, id: &MeetingId) -> Result<()> {
        let m = self
            .get(id)
            .await
            .ok_or_else(|| anyhow!("meeting not found"))?;

        // Refuse to start twice.
        if m.task.lock().await.is_some() {
            return Err(anyhow!("meeting already started"));
        }

        let api_key = std::env::var("YOGURT_DEEPGRAM_API_KEY")
            .context("YOGURT_DEEPGRAM_API_KEY not set — required for cloud STT in Phase 3")?;

        // Open audio capture on a dedicated std::thread.
        //
        // The returned `AudioStream` holds `cpal::Stream` which is `!Send`,
        // so we cannot move it into a tokio task or across an await. We
        // instead spin up a dedicated OS thread that owns the AudioStream
        // for the meeting's lifetime; the thread blocks on a oneshot until
        // the supervisor signals shutdown (via abort → drop → channel close).
        //
        // The tokio side only receives the two `broadcast::Receiver<Frame>`s
        // (which are `Send`) over a oneshot, plus the readiness signal that
        // capture opened successfully (so we can surface a permission/SCK
        // failure as a clean 400 instead of a silent task-panic).
        let (ready_tx, ready_rx) = oneshot::channel::<
            Result<(
                broadcast::Receiver<yogurt_audio::Frame>,
                broadcast::Receiver<yogurt_audio::Frame>,
            )>,
        >();
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

        std::thread::spawn(move || {
            let stream = match yogurt_audio::start_capture() {
                Ok(s) => s,
                Err(e) => {
                    let _ = ready_tx.send(Err(anyhow::Error::from(e).context(
                        "failed to open audio capture (check Screen Recording permission)",
                    )));
                    return;
                }
            };
            let mic_rx = stream.subscribe_mic();
            let sys_rx = stream.subscribe_system();
            if ready_tx.send(Ok((mic_rx, sys_rx))).is_err() {
                // Supervisor task vanished before we reported ready — just drop
                // the stream (RAII stops capture) and exit.
                return;
            }
            // Block until the supervisor drops `shutdown_tx`. We use
            // `blocking_recv` semantics via a busy-blocking wait by
            // converting the oneshot to a blocking receive.
            let _ = shutdown_rx.blocking_recv();
            // Dropping `stream` here stops both mic + system capture via RAII.
            drop(stream);
        });

        // Await capture readiness — propagates audio open failure as Result.
        let (mut mic_rx, mut sys_rx) = ready_rx
            .await
            .context("audio capture thread exited before reporting readiness")?
            .context("audio capture failed to open")?;

        let audio_tx = m.audio_tx.clone();
        let transcript_tx = m.transcript_tx.clone();
        // STT subscribes BEFORE the adapter task starts publishing, so no
        // mic chunks are dropped on the wire before Deepgram's WS is ready.
        let audio_rx_for_stt = m.audio_tx.subscribe();

        let task = tokio::spawn(async move {
            // Hold the shutdown sender until this task ends; dropping it
            // wakes the std::thread's blocking_recv, which then drops the
            // AudioStream (RAII stops cpal + SCK).
            let _shutdown_tx = shutdown_tx;

            // Spawn the STT engine first (so it subscribes before audio flows).
            let stt = Arc::new(DeepgramStt::new(api_key));
            let stt2 = stt.clone();
            let stt_handle = tokio::spawn(async move {
                if let Err(e) = stt2.start(audio_rx_for_stt, transcript_tx).await {
                    tracing::error!(?e, "stt session failed");
                }
            });

            // Adapter loop: drain both Frame receivers, convert each into
            // an AudioChunk tagged with the source channel, and publish on
            // the meeting's audio broadcast for the STT engine. Lagged
            // receivers warn + continue; closed = stream ended (RAII drop).
            loop {
                tokio::select! {
                    res = mic_rx.recv() => match res {
                        Ok(frame) => {
                            let chunk = AudioChunk {
                                channel: Channel::Mic,
                                samples: frame.samples,
                                ts_ms: frame.monotonic_micros / 1_000,
                            };
                            // send returns Err when there are no receivers
                            // (STT session ended); that's a clean termination.
                            if audio_tx.send(chunk).is_err() {
                                tracing::info!("audio adapter: no receivers, terminating");
                                break;
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!(n, "audio adapter: mic lagged");
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            tracing::info!("audio adapter: mic stream closed");
                            break;
                        }
                    },
                    res = sys_rx.recv() => match res {
                        Ok(frame) => {
                            let chunk = AudioChunk {
                                channel: Channel::System,
                                samples: frame.samples,
                                ts_ms: frame.monotonic_micros / 1_000,
                            };
                            if audio_tx.send(chunk).is_err() {
                                tracing::info!("audio adapter: no receivers, terminating");
                                break;
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!(n, "audio adapter: system lagged");
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            tracing::info!("audio adapter: system stream closed");
                            break;
                        }
                    },
                }
            }

            stt_handle.abort();
        });

        *m.task.lock().await = Some(task);
        Ok(())
    }

    /// Stop recording. Idempotent — calling on an already-stopped meeting is a no-op.
    pub async fn stop(&self, id: &MeetingId) -> Result<()> {
        let m = self
            .get(id)
            .await
            .ok_or_else(|| anyhow!("meeting not found"))?;
        if let Some(t) = m.task.lock().await.take() {
            t.abort();
        }
        Ok(())
    }

    /// Subscribe to the meeting's transcript broadcast.
    ///
    /// The receiver will see all events emitted from the subscribe call
    /// onward; older events are not replayed (matches PRD §5.2 — late
    /// joiners just see the live tail, not the meeting history).
    pub async fn subscribe(&self, id: &MeetingId) -> Option<broadcast::Receiver<TranscriptEvent>> {
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
