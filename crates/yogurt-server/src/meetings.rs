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
use tokio::sync::{broadcast, mpsc, oneshot, Mutex, RwLock};
use tokio::task::JoinHandle;
use uuid::Uuid;
use yogurt_stt::{deepgram::DeepgramStt, AudioChunk, Channel, Stt, TranscriptEvent};

// Phase 8 (Plan 08-03): `select_stt` returns a description of which
// adapter to construct.  The async caller turns the spec into an actual
// `Arc<dyn Stt>` — the local branch wraps `WhisperLocal::load` in
// `spawn_blocking` (LOCAL-05) and the cloud branch instantiates
// `DeepgramStt` directly.  Splitting the decision into a sync helper
// keeps the branch unit-testable without mocking broadcast channels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SttSpec {
    /// Cloud STT via Deepgram — needs a `YOGURT_DEEPGRAM_API_KEY`.
    Cloud { api_key: String },
    /// Local STT via whisper.cpp — needs a verified model file on disk.
    Local { model_path: std::path::PathBuf },
}

/// Lightweight projection of the fields `select_stt` cares about, so
/// tests can construct a minimal value without depending on the full
/// `yogurt_db::settings::General`.
#[derive(Debug, Clone, Default)]
pub struct SttSettings {
    pub stt_provider: String,
    pub stt_model: String,
    /// `Some(key)` when an explicit key was supplied (e.g. for cloud).
    /// Tests pass `None` to assert the cloud branch falls back to
    /// `YOGURT_DEEPGRAM_API_KEY`.
    pub deepgram_api_key: Option<String>,
}

impl From<&yogurt_db::settings::General> for SttSettings {
    fn from(g: &yogurt_db::settings::General) -> Self {
        Self {
            stt_provider: g.stt_provider.clone(),
            stt_model: g.stt_model.clone(),
            deepgram_api_key: std::env::var("YOGURT_DEEPGRAM_API_KEY").ok(),
        }
    }
}

/// Phase 8 (Plan 08-03): sync branch helper.  Returns the description
/// of which STT adapter `start()` should construct without doing any
/// IO — the caller is responsible for `WhisperLocal::load` on
/// `spawn_blocking` or `DeepgramStt::new` on the tokio task.
///
/// Errors:
/// - unknown `stt_provider` (anything other than `"cloud"` / `"local"`)
/// - `stt_model` not found in `yogurt_stt::models::REGISTRY`
/// - local model not downloaded (checked via `models::is_downloaded`,
///   which reads the sidecar `.sha256` marker; it only hashes the file
///   on the one-time legacy migration - a corrupt file counts as not
///   downloaded). Because that migration hash can take a minute on a
///   multi-GB model, async callers must invoke this fn via
///   `tokio::task::spawn_blocking`.
/// - cloud branch + no `YOGURT_DEEPGRAM_API_KEY`
pub fn select_stt(s: &SttSettings) -> Result<SttSpec> {
    match s.stt_provider.as_str() {
        "cloud" => {
            let api_key = s.deepgram_api_key.clone().ok_or_else(|| {
                anyhow!("YOGURT_DEEPGRAM_API_KEY not set — required for cloud STT")
            })?;
            Ok(SttSpec::Cloud { api_key })
        }
        "local" => {
            let spec = yogurt_stt::models::lookup(&s.stt_model)
                .ok_or_else(|| anyhow!("unknown local stt model: {}", s.stt_model))?;
            if !yogurt_stt::models::is_downloaded(spec) {
                return Err(anyhow!(
                    "local stt model {} is not downloaded; \
                     download it from Settings → Transcription → Local",
                    s.stt_model
                ));
            }
            let model_path = yogurt_stt::models::model_path(spec)
                .map_err(|e| anyhow!("resolve model path: {e}"))?;
            Ok(SttSpec::Local { model_path })
        }
        other => Err(anyhow!("unknown stt_provider: {other}")),
    }
}

pub type MeetingId = Uuid;

/// Command sent across the tokio → capture-`std::thread` bridge to hot-swap
/// the mic device mid-recording. Serviced by `run_capture_control_loop`
/// alongside the existing shutdown `oneshot`.
pub enum AudioCommand {
    SwitchMicDevice {
        device_name: String,
        reply: oneshot::Sender<std::result::Result<String, String>>,
    },
}

/// Errors `Registry::switch_mic_device` can return, mapped to HTTP status
/// codes by the routes.rs handler.
#[derive(Debug)]
pub enum SwitchDeviceError {
    NotFound,
    NotRecording,
    Device(String),
}

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
    /// Phase 4: per-meeting JSON event broadcast for non-transcript meeting
    /// events that ride on the same WS as transcripts (currently:
    /// `enhance_progress`). The `ws_meeting_handler` forwards every value
    /// sent here as a `Message::Text(json)` so the browser can observe
    /// enhance phase transitions (`sending` → `streaming` → `done`) in
    /// real time. Capacity 64 — enhance emits ~3 events per meeting; the
    /// cushion absorbs slow consumers without blocking the writer.
    pub events_tx: broadcast::Sender<serde_json::Value>,
    /// `Some` while recording, `None` before start / after stop.
    pub task: Mutex<Option<JoinHandle<()>>>,
    /// BL-05: handle to the std::thread that owns the !Send `AudioStream`.
    /// `Registry::stop` joins this with a watchdog timeout so the
    /// AudioStream Drop (which takes ~50 ms for SCK + cpal teardown) is
    /// observably complete before a subsequent `start()` can open a new
    /// SCK session. Without this join, back-to-back start/stop calls
    /// would let two AudioStreams hold SCK + cpal handles simultaneously.
    pub capture_thread: Mutex<Option<std::thread::JoinHandle<()>>>,
    /// `Some` while recording — the sending half of the capture thread's
    /// command channel, used by `Registry::switch_mic_device` to forward
    /// hot-swap requests. `None` before start / after stop, so a switch
    /// request racing with shutdown reliably observes "not recording".
    pub audio_cmd_tx: Mutex<Option<mpsc::Sender<AudioCommand>>>,
}

impl Meeting {
    fn new() -> Self {
        let (audio_tx, _) = broadcast::channel(256);
        let (transcript_tx, _) = broadcast::channel(256);
        let (events_tx, _) = broadcast::channel(64);
        Self {
            id: Uuid::now_v7(),
            created_at_ms: now_ms(),
            audio_tx,
            transcript_tx,
            events_tx,
            task: Mutex::new(None),
            capture_thread: Mutex::new(None),
            audio_cmd_tx: Mutex::new(None),
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

    /// HI-9: Re-hydrate a known-to-exist meeting (caller has verified the
    /// SQLite row) into a fresh in-memory Meeting. The audio/transcript
    /// broadcasts are empty (recording is over by definition — the meeting
    /// only got persisted to SQLite if `enhance` ran), but `events_tx` is
    /// live so subsequent Re-enhance calls can still broadcast progress to
    /// any WS subscribers.
    ///
    /// Uses `id` (the caller-provided UUID) rather than minting a new one
    /// so the in-memory copy matches the SQLite row's id.
    pub async fn hydrate(&self, id: MeetingId) -> Arc<Meeting> {
        let mut guard = self.meetings.write().await;
        // Double-check inside the lock — another concurrent enhance handler
        // may have hydrated the same meeting between our SELECT and our
        // write-lock acquisition.
        if let Some(existing) = guard.get(&id) {
            return existing.clone();
        }
        let mut m = Meeting::new();
        m.id = id;
        let m = Arc::new(m);
        guard.insert(id, m.clone());
        m
    }

    /// Start recording: spin up `yogurt-audio` capture + the configured
    /// STT engine (cloud Deepgram or local WhisperLocal).
    ///
    /// Phase 8 (Plan 08-03) refactor: the adapter selection now branches
    /// on `settings.stt_provider` via the `select_stt` helper.  Cloud is
    /// the seed default (V005), so existing user DBs keep using
    /// Deepgram with no behavior change.
    ///
    /// Errors:
    ///   - meeting not found
    ///   - meeting already started
    ///   - `select_stt` rejected the settings (unknown provider,
    ///     missing API key, model not downloaded, …)
    ///   - `yogurt_audio::start_capture()` failed (caller surfaces as
    ///     a 400 — typically a permission gate)
    pub async fn start(
        &self,
        id: &MeetingId,
        stt_settings: SttSettings,
        mic_device: Option<String>,
    ) -> Result<()> {
        let m = self
            .get(id)
            .await
            .ok_or_else(|| anyhow!("meeting not found"))?;

        // Refuse to start twice.
        if m.task.lock().await.is_some() {
            return Err(anyhow!("meeting already started"));
        }

        // select_stt probes the model file on disk (and may pay a
        // one-time legacy hash of a multi-GB file) - keep it off the
        // tokio workers.
        let stt_spec = tokio::task::spawn_blocking(move || select_stt(&stt_settings))
            .await
            .context("join select_stt")?
            .context("select STT adapter")?;

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
        let (cmd_tx, cmd_rx) = mpsc::channel::<AudioCommand>(4);

        // BL-05: capture the JoinHandle so `stop()` can wait for the
        // AudioStream Drop (~50 ms SCK + cpal teardown) to complete before
        // a subsequent `start()` can re-enter. Detached threads led to
        // overlapping SCK sessions on back-to-back start/stop calls.
        //
        // Panic surface: the thread body is wrapped in catch_unwind so a
        // panic inside `yogurt_audio::start_capture()` (most commonly the
        // `screencapturekit` crate panicking on a TCC denial that wasn't
        // surfaced via the preflight check) is converted into a clean
        // `Err` on `ready_tx`. Without this, a panic dropped `ready_tx`
        // silently and the supervisor saw `RecvError::Closed`, producing
        // the unhelpful "channel closed" 400 — see the user-debug session
        // 2026-06-28 where this masked a permission failure.
        // `start_capture()` spawns tokio drainer tasks internally
        // (mic.rs / system.rs), so the capture thread must carry the
        // server runtime's context - a bare std::thread has none and
        // `tokio::spawn` panics with "no reactor running".
        let rt_handle = tokio::runtime::Handle::current();
        let capture_thread = std::thread::spawn(move || {
            let _rt_guard = rt_handle.enter();
            // Use Option so the panic-handler branch can take ready_tx
            // and send an Err.
            let mut ready_tx_slot = Some(ready_tx);
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut stream = match yogurt_audio::start_capture(mic_device.as_deref()) {
                    Ok(s) => s,
                    Err(e) => {
                        if let Some(tx) = ready_tx_slot.take() {
                            let _ = tx.send(Err(anyhow::Error::from(e).context(
                                "failed to open audio capture (check Screen Recording permission)",
                            )));
                        }
                        return;
                    }
                };
                let mic_rx = stream.subscribe_mic();
                let sys_rx = stream.subscribe_system();
                let Some(tx) = ready_tx_slot.take() else {
                    return;
                };
                if tx.send(Ok((mic_rx, sys_rx))).is_err() {
                    // Supervisor task vanished before we reported ready —
                    // just drop the stream (RAII stops capture) and exit.
                    return;
                }
                // Block until the supervisor drops `shutdown_tx`, servicing
                // hot-swap commands from `Registry::switch_mic_device` in
                // the meantime.
                run_capture_control_loop(&rt_handle, shutdown_rx, cmd_rx, |name| {
                    let opt = if name.is_empty() { None } else { Some(name) };
                    stream.switch_mic_device(opt).map_err(|e| e.to_string())
                });
                // Dropping `stream` here stops both mic + system capture via RAII.
                drop(stream);
            }));
            if let Err(payload) = result {
                // The thread body panicked. Extract a human-readable
                // message and (if we still own ready_tx) surface it as a
                // clean error so the supervisor returns a 400 with the
                // real reason instead of "channel closed".
                let msg = if let Some(s) = payload.downcast_ref::<String>() {
                    s.clone()
                } else if let Some(s) = payload.downcast_ref::<&'static str>() {
                    (*s).to_string()
                } else {
                    "unknown panic in audio capture thread".to_string()
                };
                tracing::error!(panic = %msg, "audio capture thread panicked");
                if let Some(tx) = ready_tx_slot.take() {
                    let _ = tx.send(Err(anyhow!(
                        "audio capture panicked: {msg} \
                         — usually means Screen Recording permission is missing \
                         or revoked; check System Settings → Privacy & Security \
                         → Screen Recording & System Audio"
                    )));
                }
            }
        });

        // Await capture readiness — propagates audio open failure as Result.
        let (mic_rx, sys_rx) = ready_rx
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

            // Phase 8 (Plan 08-03): construct the adapter from the spec
            // `select_stt` chose.  WhisperLocal::load is synchronous and
            // CPU-heavy — wrap in `spawn_blocking` so the tokio scheduler
            // stays responsive (LOCAL-05).
            let stt: Arc<dyn Stt> = match stt_spec {
                SttSpec::Cloud { api_key } => Arc::new(DeepgramStt::new(api_key)),
                SttSpec::Local { model_path } => {
                    // LOCAL-05: WhisperLocal::load reads ~500 MB from disk
                    // and runs ggml init; it is intentionally synchronous.
                    // spawn_blocking keeps the scheduler healthy.
                    let loaded = match tokio::task::spawn_blocking(move || {
                        yogurt_stt::WhisperLocal::load(model_path)
                    })
                    .await
                    {
                        Ok(Ok(local)) => local,
                        Ok(Err(e)) => {
                            tracing::error!(?e, "WhisperLocal::load failed");
                            return;
                        }
                        Err(e) => {
                            tracing::error!(
                                ?e,
                                "spawn_blocking join failed for WhisperLocal::load"
                            );
                            return;
                        }
                    };
                    Arc::new(loaded)
                }
            };

            // Spawn the STT engine first (so it subscribes before audio flows).
            let stt2 = stt.clone();
            let stt_handle = tokio::spawn(async move {
                if let Err(e) = stt2.start(audio_rx_for_stt, transcript_tx).await {
                    tracing::error!(?e, "stt session failed");
                }
            });

            pump_audio_adapter(mic_rx, sys_rx, audio_tx).await;
            stt_handle.abort();
        });

        *m.task.lock().await = Some(task);
        *m.capture_thread.lock().await = Some(capture_thread);
        *m.audio_cmd_tx.lock().await = Some(cmd_tx);
        Ok(())
    }

    /// Stop recording. Idempotent — calling on an already-stopped meeting is a no-op.
    ///
    /// BL-05: after aborting the supervisor task (which drops `_shutdown_tx`,
    /// waking the std::thread's `blocking_recv` and triggering
    /// `drop(stream)`), we join the capture thread with a 200ms watchdog so
    /// AudioStream Drop completes before a subsequent `start()` opens a new
    /// SCK session. The join runs inside `spawn_blocking` so we don't block
    /// the tokio reactor; a timeout fires only if the thread is wedged
    /// (which would be a real bug worth surfacing).
    pub async fn stop(&self, id: &MeetingId) -> Result<()> {
        let m = self
            .get(id)
            .await
            .ok_or_else(|| anyhow!("meeting not found"))?;

        // Clear the command channel first so any switch request racing
        // with shutdown reliably observes "not recording" rather than
        // sending into a channel whose receiver is about to vanish.
        *m.audio_cmd_tx.lock().await = None;

        // Step 1: abort the tokio supervisor task. This drops `_shutdown_tx`
        // inside the task body, which wakes the std::thread's blocking_recv.
        if let Some(t) = m.task.lock().await.take() {
            t.abort();
            // Wait for the task to actually exit (abort is asynchronous);
            // its body owns `_shutdown_tx` which must drop before the
            // capture thread can wake.
            let _ = t.await;
        }

        // Step 2: join the std::thread that owns the AudioStream so the
        // SCK + cpal teardown finishes before this call returns. Use
        // spawn_blocking so we don't block the reactor; cap the wait at
        // 200ms (the docs say SCK + cpal Drop is ~50ms — 200ms is generous).
        if let Some(handle) = m.capture_thread.lock().await.take() {
            let join_result = tokio::time::timeout(
                std::time::Duration::from_millis(200),
                tokio::task::spawn_blocking(move || handle.join()),
            )
            .await;
            match join_result {
                Ok(Ok(Ok(()))) => {
                    tracing::debug!("audio capture thread joined cleanly");
                }
                Ok(Ok(Err(e))) => {
                    tracing::error!(?e, "audio capture thread panicked during shutdown");
                }
                Ok(Err(e)) => {
                    tracing::error!(?e, "spawn_blocking for capture-thread join failed");
                }
                Err(_) => {
                    // Watchdog fired — capture thread is wedged (SCK didn't
                    // release the device, or our cleanup races). Log loudly
                    // so future investigators have a signal.
                    tracing::error!(
                        "audio capture thread did not exit within 200ms — \
                         AudioStream may still hold SCK/cpal handles"
                    );
                }
            }
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

    /// Hot-swap the mic device on an actively-recording meeting. Forwards
    /// an `AudioCommand::SwitchMicDevice` into the capture thread's command
    /// channel and awaits the reply with a 5s timeout so a wedged capture
    /// thread surfaces as an error instead of hanging the request forever.
    pub async fn switch_mic_device(
        &self,
        id: &MeetingId,
        device_name: String,
    ) -> std::result::Result<String, SwitchDeviceError> {
        let m = self.get(id).await.ok_or(SwitchDeviceError::NotFound)?;

        let tx = m
            .audio_cmd_tx
            .lock()
            .await
            .clone()
            .ok_or(SwitchDeviceError::NotRecording)?;

        let (reply_tx, reply_rx) = oneshot::channel();
        tx.send(AudioCommand::SwitchMicDevice {
            device_name,
            reply: reply_tx,
        })
        .await
        .map_err(|_| SwitchDeviceError::NotRecording)?;

        match tokio::time::timeout(std::time::Duration::from_secs(5), reply_rx).await {
            Ok(Ok(Ok(name))) => Ok(name),
            Ok(Ok(Err(msg))) => Err(SwitchDeviceError::Device(msg)),
            Ok(Err(_)) => Err(SwitchDeviceError::NotRecording),
            Err(_) => Err(SwitchDeviceError::Device(
                "timed out waiting for capture thread to switch device".into(),
            )),
        }
    }
}

/// Service `AudioCommand`s (currently just `SwitchMicDevice`) from the
/// tokio side while blocking the capture `std::thread` until the supervisor
/// drops `shutdown_rx`. Runs on `rt_handle` (the same `Handle` the capture
/// thread already carries so `start_capture`'s internal `tokio::spawn`
/// drainer tasks have a reactor) via `block_on`, so this call blocks the
/// calling OS thread exactly like the old `shutdown_rx.blocking_recv()` did.
///
/// Extracted from the inline capture-thread body so the real `tokio::select!`
/// control loop is unit-testable across a real thread boundary without
/// depending on audio hardware — mirrors `pump_audio_adapter`.
fn run_capture_control_loop(
    rt_handle: &tokio::runtime::Handle,
    mut shutdown_rx: oneshot::Receiver<()>,
    mut cmd_rx: mpsc::Receiver<AudioCommand>,
    mut switch: impl FnMut(&str) -> std::result::Result<String, String>,
) {
    rt_handle.block_on(async {
        loop {
            tokio::select! {
                _ = &mut shutdown_rx => break,
                cmd = cmd_rx.recv() => match cmd {
                    Some(AudioCommand::SwitchMicDevice { device_name, reply }) => {
                        let _ = reply.send(switch(&device_name));
                    }
                    None => break,
                }
            }
        }
    })
}

/// Drain Frame receivers from `yogurt-audio` → publish `AudioChunk`s onto the
/// meeting's audio broadcast for the STT engine. Lagged receivers warn +
/// continue.
///
/// BL-04: when ONE channel closes (e.g. SCK system stream drops because no
/// app is producing audio), keep draining the OTHER channel. We only exit
/// when BOTH channels are closed, OR when the audio broadcast has no
/// receivers (STT session ended). The `if` guards on each select arm
/// disable a closed channel so we don't busy-poll it once it's done.
///
/// Extracted from the inline supervisor loop so the BL-04 behavior is unit-
/// testable without spinning up real SCK + cpal streams.
pub(crate) async fn pump_audio_adapter(
    mut mic_rx: broadcast::Receiver<yogurt_audio::Frame>,
    mut sys_rx: broadcast::Receiver<yogurt_audio::Frame>,
    audio_tx: broadcast::Sender<AudioChunk>,
) {
    let mut mic_open = true;
    let mut sys_open = true;
    while mic_open || sys_open {
        tokio::select! {
            res = mic_rx.recv(), if mic_open => match res {
                Ok(frame) => {
                    let chunk = AudioChunk {
                        channel: Channel::Mic,
                        samples: frame.samples,
                        ts_ms: frame.monotonic_micros / 1_000,
                    };
                    if audio_tx.send(chunk).is_err() {
                        tracing::info!("audio adapter: no receivers, terminating");
                        return;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(n, "audio adapter: mic lagged");
                }
                Err(broadcast::error::RecvError::Closed) => {
                    tracing::info!("audio adapter: mic stream closed — continuing with system only");
                    mic_open = false;
                }
            },
            res = sys_rx.recv(), if sys_open => match res {
                Ok(frame) => {
                    let chunk = AudioChunk {
                        channel: Channel::System,
                        samples: frame.samples,
                        ts_ms: frame.monotonic_micros / 1_000,
                    };
                    if audio_tx.send(chunk).is_err() {
                        tracing::info!("audio adapter: no receivers, terminating");
                        return;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(n, "audio adapter: system lagged");
                }
                Err(broadcast::error::RecvError::Closed) => {
                    tracing::info!("audio adapter: system stream closed — continuing with mic only");
                    sys_open = false;
                }
            },
        }
    }
    tracing::info!("audio adapter: both channels closed, terminating");
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

    /// Proves the real `tokio::select!` capture-thread control loop
    /// services multiple hot-swap commands in order and exits cleanly on
    /// shutdown, with no deadlock — entirely independent of real audio
    /// hardware (mirrors how `pump_audio_adapter` is tested). A `1s`
    /// timeout on each reply means a deadlock fails the test loudly rather
    /// than hanging the test run.
    #[test]
    fn run_capture_control_loop_services_commands_then_exits_cleanly() {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("build multi-thread runtime");
        let handle = rt.handle().clone();

        let (cmd_tx, cmd_rx) = mpsc::channel::<AudioCommand>(4);
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

        let seen: Arc<std::sync::Mutex<Vec<String>>> = Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen_for_worker = seen.clone();

        // Mirrors how `Registry::start` runs the loop: a plain
        // std::thread::spawn body that block_on's the loop on the
        // captured `Handle`.
        let worker = std::thread::spawn(move || {
            run_capture_control_loop(&handle, shutdown_rx, cmd_rx, move |name: &str| {
                seen_for_worker.lock().unwrap().push(name.to_string());
                Ok(format!("resolved:{name}"))
            });
        });

        rt.block_on(async {
            for name in ["mic-a", "mic-b", "mic-c"] {
                let (reply_tx, reply_rx) = oneshot::channel();
                cmd_tx
                    .send(AudioCommand::SwitchMicDevice {
                        device_name: name.to_string(),
                        reply: reply_tx,
                    })
                    .await
                    .expect("send command");
                let reply = tokio::time::timeout(std::time::Duration::from_secs(1), reply_rx)
                    .await
                    .expect("reply within 1s — a timeout means the select loop deadlocked")
                    .expect("reply sender dropped unexpectedly");
                assert_eq!(reply, Ok(format!("resolved:{name}")));
            }
        });

        // Signal shutdown and join — a hang here means the loop can't
        // observe the dropped oneshot and exit cleanly.
        drop(shutdown_tx);
        worker.join().expect("worker thread joins cleanly");

        assert_eq!(
            *seen.lock().unwrap(),
            vec![
                "mic-a".to_string(),
                "mic-b".to_string(),
                "mic-c".to_string()
            ]
        );
    }

    /// BL-04: when the system channel closes, the adapter must KEEP draining
    /// mic frames — not exit. The old behavior `break`'d out of the loop on
    /// the first `Closed`, which silently dropped mic audio for the rest of
    /// the meeting whenever no app was producing system audio.
    #[tokio::test]
    async fn it_continues_mic_when_system_channel_closes() {
        let (mic_tx, mic_rx) = broadcast::channel::<yogurt_audio::Frame>(16);
        let (sys_tx, sys_rx) = broadcast::channel::<yogurt_audio::Frame>(16);
        let (audio_tx, mut audio_rx) = broadcast::channel::<AudioChunk>(16);

        // Drop the system sender immediately → sys_rx will see Closed on
        // its first recv.
        drop(sys_tx);

        let pump = tokio::spawn(pump_audio_adapter(mic_rx, sys_rx, audio_tx));

        // Give the adapter a beat to observe sys closed.
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;

        // Now push 3 mic frames; they MUST survive even though sys is gone.
        for i in 0..3 {
            mic_tx
                .send(yogurt_audio::Frame {
                    channel: yogurt_audio::Channel::Mic,
                    samples: vec![0i16; 320],
                    monotonic_micros: (i as u64) * 20_000,
                })
                .unwrap();
        }

        for expected_ts in [0u64, 20, 40] {
            let chunk =
                tokio::time::timeout(std::time::Duration::from_millis(500), audio_rx.recv())
                    .await
                    .expect("chunk within 500ms")
                    .expect("recv ok");
            assert_eq!(chunk.channel, Channel::Mic);
            assert_eq!(chunk.ts_ms, expected_ts);
        }

        // Closing mic too must terminate the pump cleanly.
        drop(mic_tx);
        tokio::time::timeout(std::time::Duration::from_secs(1), pump)
            .await
            .expect("pump should exit when both channels closed")
            .expect("pump task joined");
    }

    /// Symmetric BL-04 case: mic closes first, system survives.
    #[tokio::test]
    async fn it_continues_system_when_mic_channel_closes() {
        let (mic_tx, mic_rx) = broadcast::channel::<yogurt_audio::Frame>(16);
        let (sys_tx, sys_rx) = broadcast::channel::<yogurt_audio::Frame>(16);
        let (audio_tx, mut audio_rx) = broadcast::channel::<AudioChunk>(16);

        drop(mic_tx);

        let pump = tokio::spawn(pump_audio_adapter(mic_rx, sys_rx, audio_tx));
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;

        sys_tx
            .send(yogurt_audio::Frame {
                channel: yogurt_audio::Channel::System,
                samples: vec![0i16; 320],
                monotonic_micros: 5_000,
            })
            .unwrap();

        let chunk = tokio::time::timeout(std::time::Duration::from_millis(500), audio_rx.recv())
            .await
            .expect("chunk within 500ms")
            .expect("recv ok");
        assert_eq!(chunk.channel, Channel::System);
        assert_eq!(chunk.ts_ms, 5);

        drop(sys_tx);
        tokio::time::timeout(std::time::Duration::from_secs(1), pump)
            .await
            .expect("pump should exit when both channels closed")
            .expect("pump task joined");
    }

    /// Sanity: pump exits cleanly when BOTH channels close from the start.
    #[tokio::test]
    async fn it_exits_when_both_channels_closed() {
        let (mic_tx, mic_rx) = broadcast::channel::<yogurt_audio::Frame>(16);
        let (sys_tx, sys_rx) = broadcast::channel::<yogurt_audio::Frame>(16);
        let (audio_tx, _audio_rx) = broadcast::channel::<AudioChunk>(16);

        drop(mic_tx);
        drop(sys_tx);

        let pump = tokio::spawn(pump_audio_adapter(mic_rx, sys_rx, audio_tx));
        tokio::time::timeout(std::time::Duration::from_secs(1), pump)
            .await
            .expect("pump must exit promptly when both channels closed")
            .expect("pump task joined");
    }

    // ─── Phase 8 (Plan 08-03) select_stt branch coverage ────────────────────

    /// Anything other than "cloud" / "local" is a hard error.  Catches
    /// typos in the Settings page (e.g. "Local" with a capital L) before
    /// they reach the audio pipeline.
    #[test]
    fn rejects_unknown_provider() {
        let s = SttSettings {
            stt_provider: "satellite".into(),
            stt_model: "small.en".into(),
            deepgram_api_key: Some("dummy".into()),
        };
        let err = select_stt(&s).unwrap_err();
        assert!(
            format!("{err:#}").contains("unknown stt_provider"),
            "got: {err:#}"
        );
    }

    /// Local branch with a model name that isn't in REGISTRY is a hard
    /// error — better than spawning a download for a typo.
    #[test]
    fn rejects_local_when_model_missing() {
        let s = SttSettings {
            stt_provider: "local".into(),
            stt_model: "ghost.en".into(),
            deepgram_api_key: None,
        };
        let err = select_stt(&s).unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("unknown") || msg.contains("not downloaded"),
            "got: {msg}"
        );
    }

    /// Cloud branch requires an API key.  The legacy callers used to
    /// surface `YOGURT_DEEPGRAM_API_KEY not set` directly; the new
    /// helper carries the same intent through `SttSettings`.
    #[test]
    fn rejects_cloud_without_key() {
        let s = SttSettings {
            stt_provider: "cloud".into(),
            stt_model: "small.en".into(),
            deepgram_api_key: None,
        };
        let err = select_stt(&s).unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("YOGURT_DEEPGRAM_API_KEY") || msg.contains("api key"),
            "got: {msg}"
        );
    }

    /// Cloud branch with a key succeeds.
    #[test]
    fn accepts_cloud_with_key() {
        let s = SttSettings {
            stt_provider: "cloud".into(),
            stt_model: "small.en".into(),
            deepgram_api_key: Some("dg_xxx".into()),
        };
        let spec = select_stt(&s).expect("cloud + key should succeed");
        assert_eq!(
            spec,
            SttSpec::Cloud {
                api_key: "dg_xxx".into()
            }
        );
    }
}
