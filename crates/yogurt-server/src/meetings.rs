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
//!   1. `start()` reads `YOGURT_DEEPGRAM_API_KEY` (D-07; Phase 5 swaps to the key file).
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
use std::collections::{HashMap, VecDeque};
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
                anyhow!(
                    "no Deepgram API key configured — add one in Settings → \
                     Transcription, or switch to local transcription"
                )
            })?;
            Ok(SttSpec::Cloud { api_key })
        }
        "local" => {
            let spec = yogurt_stt::models::lookup(&s.stt_model)
                .ok_or_else(|| anyhow!("unknown local stt model: {}", s.stt_model))?;
            // One resolution for both questions - "is it here?" and
            // "where?" - so the two can't disagree, and so a model
            // installed by the Homebrew companion formula (AUD-4) loads
            // from its prefix instead of reading as not-downloaded.
            let model_path = yogurt_stt::models::resolve_model(spec).ok_or_else(|| {
                anyhow!(
                    "local stt model {} is not downloaded; \
                     download it from Settings → Transcription → Local",
                    s.stt_model
                )
            })?;
            Ok(SttSpec::Local { model_path })
        }
        other => Err(anyhow!("unknown stt_provider: {other}")),
    }
}

/// Well-known `ApiKeyStore` entry id for the Deepgram STT key. Written by
/// the Settings → Transcription UI (and the `.env.local` bootstrap seeder);
/// read by `routes::start_meeting` when the env var override is absent.
pub const DEEPGRAM_KEY_ID: &str = "stt-deepgram";

pub type MeetingId = Uuid;

/// Command sent across the tokio → capture-`std::thread` bridge to hot-swap
/// the mic device or pause/resume it mid-recording. Serviced by
/// `run_capture_control_loop` alongside the existing shutdown `oneshot`.
pub enum AudioCommand {
    /// Replies with the resolved device name and whether the mic echo is
    /// still live after the swap.
    SwitchMicDevice {
        device_name: String,
        reply: oneshot::Sender<std::result::Result<(String, bool), String>>,
    },
    /// AUD-6: pause (`true`) or resume (`false`) mic audio reaching the STT
    /// pipeline, without touching system audio or restarting capture.
    SetMicMuted {
        muted: bool,
        reply: oneshot::Sender<std::result::Result<(), String>>,
    },
    /// Open/close/hot-swap the mic echo. `enabled: false` closes it
    /// (device/buffer ignored); `enabled: true` opens or re-opens it on
    /// `device_name` (`""` = system default) at `buffer` frames.
    SetEcho {
        enabled: bool,
        device_name: String,
        buffer: u32,
        reply: oneshot::Sender<std::result::Result<(bool, String), String>>,
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
    /// seconds of 20ms chunks; lagged subscribers drop frames (logged at debug).
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
    /// `Some` while recording — shutdown signal + JoinHandle for the
    /// transcript-persistence task spawned by `Registry::start`. `stop()`
    /// signals it, then awaits the handle so the final SQLite write is
    /// observably complete before the stop request returns (the browser
    /// navigates to the post-meeting view immediately after).
    pub persist: Mutex<Option<(oneshot::Sender<()>, JoinHandle<()>)>>,
    /// Which STT engine `select_stt` actually resolved to for the
    /// in-progress (or most recent) recording — `"cloud"` or `"local"`.
    /// `None` before the first start. Set once, right after `select_stt`
    /// resolves in `Registry::start`, so it reflects truth even if the
    /// user flips Settings mid-recording (settings only apply at the
    /// *next* start — see settings.rs's PATCH validation). Read by the
    /// `GET /api/meetings/active` route for the live engine badge.
    pub stt_engine: Mutex<Option<&'static str>>,
    /// AUD-6: whether the mic is currently paused. `false` before the first
    /// start and reset to `false` on every new start (mirrors `MicCapture`
    /// itself never carrying mute state across a stop/start). Set by
    /// `Registry::set_mic_muted` after the capture thread confirms the
    /// change; read by the `GET /api/meetings/active` route so a reload or
    /// second tab reflects the true state without new WS plumbing.
    pub mic_muted: Mutex<bool>,
    /// Whether the mic is currently being echoed to an output device.
    /// Same lifecycle as `mic_muted`.
    pub echo_enabled: Mutex<bool>,
}

/// Abort a spawned task when the owner is dropped. Used so the STT session
/// and the whisper partial ticker cannot outlive the meeting: aborting the
/// supervisor task drops its locals, which aborts the children in turn.
/// (JoinHandle's own Drop detaches instead of aborting — that detachment is
/// exactly the leak this guard exists to prevent.)
struct AbortOnDrop(JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
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
            persist: Mutex::new(None),
            stt_engine: Mutex::new(None),
            mic_muted: Mutex::new(false),
            echo_enabled: Mutex::new(false),
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
    /// Serializes `start()` calls so the single-active-recording invariant
    /// cannot be raced by two concurrent /start requests for different
    /// meetings (each would otherwise pass the active check before the
    /// other claimed its task slot).
    start_gate: Mutex<()>,
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

    /// Return the id of the single currently-recording meeting, if any.
    /// "Recording" means `task` is `Some` (the audio + STT supervisor task
    /// is live). Backs the floating "Return to recording" pill (GET
    /// `/api/meetings/active`), which the frontend polls every 5s.
    ///
    /// ponytail: O(n) scan + a `task` mutex lock per entry. Fine at this
    /// scale — a handful of in-memory meetings, one real recording at a
    /// time (single mic). Upgrade to a tracked "active id" field only if
    /// the registry ever grows large enough for this to show up in a
    /// profile.
    pub async fn active_recording(&self) -> Option<MeetingId> {
        let meetings = self.meetings.read().await;
        for (id, m) in meetings.iter() {
            if m.task.lock().await.is_some() {
                return Some(*id);
            }
        }
        None
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
        repo: Option<Arc<yogurt_db::MeetingRepo>>,
    ) -> Result<()> {
        let m = self
            .get(id)
            .await
            .ok_or_else(|| anyhow!("meeting not found"))?;

        // Single-active-recording invariant: one meeting records at a
        // time, matching the product model (and what the return-pill /
        // library-badge UI assumes). The gate serializes concurrent
        // start() calls so the check below cannot be raced.
        let _gate = self.start_gate.lock().await;
        if let Some(active) = self.active_recording().await {
            if active == *id {
                return Err(anyhow!("meeting already started"));
            }
            return Err(anyhow!(
                "another meeting is already recording - stop or end it first"
            ));
        }

        // Refuse to start twice. Hold the task lock for the whole start
        // sequence so two concurrent /start calls can't both pass this
        // check and open two SCK capture sessions (the loser used to leak
        // its session forever).
        let mut task_slot = m.task.lock().await;
        if task_slot.is_some() {
            return Err(anyhow!("meeting already started"));
        }

        // select_stt probes the model file on disk (and may pay a
        // one-time legacy hash of a multi-GB file) - keep it off the
        // tokio workers.
        // Two short strings for the meeting_started line below; the settings
        // themselves move into the closure on the next line.
        let (stt_provider, stt_model) = (
            stt_settings.stt_provider.clone(),
            stt_settings.stt_model.clone(),
        );
        let stt_spec = tokio::task::spawn_blocking(move || select_stt(&stt_settings))
            .await
            .context("join select_stt")?
            .context("select STT adapter")?;

        // Record which engine actually won, before spawning anything —
        // this is the truthful source for the live-header badge (D-XX:
        // settings only take effect at the *next* start, so a mid-
        // recording Settings flip must not lie about what's running).
        *m.stt_engine.lock().await = Some(match stt_spec {
            SttSpec::Cloud { .. } => "cloud",
            SttSpec::Local { .. } => "local",
        });

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
                // hot-swap / mute commands from `Registry::switch_mic_device`
                // and `Registry::set_mic_muted` in the meantime. One dispatch
                // closure (rather than one per command) because both variants
                // need `stream` and two closures can't each hold their own
                // borrow of it at once.
                run_capture_control_loop(&rt_handle, shutdown_rx, cmd_rx, |cmd| match cmd {
                    AudioCommand::SwitchMicDevice { device_name, reply } => {
                        let opt = if device_name.is_empty() {
                            None
                        } else {
                            Some(device_name.as_str())
                        };
                        let _ = reply.send(
                            stream
                                .switch_mic_device(opt)
                                .map(|name| (name, stream.echo_device().is_some()))
                                .map_err(|e| e.to_string()),
                        );
                    }
                    AudioCommand::SetMicMuted { muted, reply } => {
                        stream.set_mic_muted(muted);
                        let _ = reply.send(Ok(()));
                    }
                    AudioCommand::SetEcho {
                        enabled,
                        device_name,
                        buffer,
                        reply,
                    } => {
                        if !enabled {
                            stream.stop_echo();
                            let _ = reply.send(Ok((false, String::new())));
                        } else {
                            let dev = if device_name.is_empty() {
                                None
                            } else {
                                Some(device_name.as_str())
                            };
                            let _ = reply.send(
                                stream
                                    .start_echo(dev, buffer)
                                    .map(|name| (true, name))
                                    .map_err(|e| e.to_string()),
                            );
                        }
                    }
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
        let events_tx = m.events_tx.clone();
        // STT subscribes BEFORE the adapter task starts publishing, so no
        // mic chunks are dropped on the wire before Deepgram's WS is ready.
        let audio_rx_for_stt = m.audio_tx.subscribe();

        // Continuation-session clock: elapsed wall-clock ms since this
        // meeting's ORIGINAL started_at, or 0 for a genuine first session.
        // `routes::start_meeting` preserves the original started_at on a
        // restart (see `start_stamp_patch`), so this reads the true
        // session-1 start time even on session 2+. Added to every
        // TranscriptEvent's ts_ms by `relay_transcript_events` below so a
        // stop/restart within one meeting keeps producing monotonically
        // increasing timestamps instead of each session restarting its
        // clock at 0 and colliding with (or preceding) the prior one.
        let offset_ms = session_offset_ms(repo.as_ref(), &m.id.to_string()).await;

        // STT publishes onto this private channel rather than
        // `transcript_tx` directly so `relay_transcript_events` can apply
        // `offset_ms` in exactly ONE place before either consumer of
        // `transcript_tx` (WS clients via `Registry::subscribe`, and
        // `persist_transcript` below) ever sees the event.
        let (raw_transcript_tx, raw_transcript_rx) = broadcast::channel::<TranscriptEvent>(256);

        // Transcript persistence: subscribe before STT starts so no final
        // segment can slip past, then accumulate finals into the meeting's
        // SQLite row as they arrive. This is the source of truth that
        // enhance, chat, and the post-meeting transcript panel all read —
        // the browser never carries the transcript itself.
        if let Some(repo) = repo {
            let persist_rx = m.transcript_tx.subscribe();
            let (persist_shutdown_tx, persist_shutdown_rx) = oneshot::channel::<()>();
            let handle = tokio::spawn(persist_transcript(
                repo,
                m.id.to_string(),
                persist_rx,
                persist_shutdown_rx,
            ));
            *m.persist.lock().await = Some((persist_shutdown_tx, handle));
        }

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
                            send_stt_error(
                                &events_tx,
                                &format!(
                                    "Local transcription failed to load its model: {e:#}. \
                                     Re-download it from Settings → Transcription."
                                ),
                            );
                            return;
                        }
                        Err(e) => {
                            tracing::error!(
                                ?e,
                                "spawn_blocking join failed for WhisperLocal::load"
                            );
                            send_stt_error(&events_tx, "Local transcription failed to start.");
                            return;
                        }
                    };
                    Arc::new(loaded)
                }
            };

            // Relay raw STT output onto the meeting's public `transcript_tx`,
            // applying `offset_ms` in exactly one place (see the comment
            // above `session_offset_ms`, above). Guarded by AbortOnDrop the
            // same way as the STT session below, so aborting this supervisor
            // tears the relay down too — otherwise it would keep running
            // (forever awaiting a raw channel whose only sender is about to
            // be dropped with the STT task) until that drop finally closed
            // it, an indirect and unnecessarily delayed shutdown.
            let relay_guard = AbortOnDrop(tokio::spawn(relay_transcript_events(
                raw_transcript_rx,
                transcript_tx,
                offset_ms,
            )));

            // Spawn the STT engine first (so it subscribes before audio
            // flows). The AbortOnDrop guard ties the STT session's lifetime
            // to this supervisor: when `stop()` aborts us, the guard drops
            // and aborts the STT task too. Without it, Deepgram/Whisper
            // sessions outlived the meeting (Deepgram kept its WS open;
            // WhisperLocal pinned a multi-GB model and kept decoding).
            let stt2 = stt.clone();
            let events_tx_for_stt = events_tx.clone();
            let stt_guard = AbortOnDrop(tokio::spawn(async move {
                if let Err(e) = stt2.start(audio_rx_for_stt, raw_transcript_tx).await {
                    tracing::error!(?e, "stt session failed");
                    send_stt_error(&events_tx_for_stt, &format!("Transcription stopped: {e:#}"));
                }
            }));

            pump_audio_adapter(mic_rx, sys_rx, audio_tx, events_tx).await;
            drop(stt_guard);
            drop(relay_guard);
        });

        *task_slot = Some(task);
        // Release the task lock BEFORE acquiring the sibling mutexes —
        // `stop()` takes `audio_cmd_tx` first, so holding `task` across
        // these acquisitions would be an ABBA deadlock window.
        drop(task_slot);
        *m.capture_thread.lock().await = Some(capture_thread);
        *m.audio_cmd_tx.lock().await = Some(cmd_tx);
        tracing::info!(
            event = "meeting_started",
            meeting_id = %id,
            stt_provider = %stt_provider,
            stt_model = %stt_model,
            "meeting started"
        );
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

        // Step 3: flush the transcript-persistence task. Signal shutdown
        // (it drains any still-queued broadcast events, does a final SQLite
        // write, then exits) and await it so the post-meeting view — which
        // the browser opens immediately after this request returns — reads
        // the complete transcript.
        if let Some((shutdown_tx, handle)) = m.persist.lock().await.take() {
            let _ = shutdown_tx.send(());
            match tokio::time::timeout(std::time::Duration::from_secs(2), handle).await {
                Ok(_) => tracing::debug!("transcript persistence flushed"),
                Err(_) => {
                    tracing::error!("transcript persistence did not flush within 2s");
                }
            }
        }

        tracing::info!(event = "meeting_stopped", meeting_id = %id, "meeting stopped");
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
            Ok(Ok(Ok((name, echo_live)))) => {
                *m.echo_enabled.lock().await = echo_live;
                Ok(name)
            }
            Ok(Ok(Err(msg))) => Err(SwitchDeviceError::Device(msg)),
            Ok(Err(_)) => Err(SwitchDeviceError::NotRecording),
            Err(_) => Err(SwitchDeviceError::Device(
                "timed out waiting for capture thread to switch device".into(),
            )),
        }
    }

    /// AUD-6: pause or resume the mic on an actively-recording meeting.
    /// Same shape as `switch_mic_device` — forwards an
    /// `AudioCommand::SetMicMuted` into the capture thread and awaits the
    /// reply with a 5s timeout — plus stamps `Meeting::mic_muted` on
    /// success so `GET /api/meetings/active` reflects the true state.
    pub async fn set_mic_muted(
        &self,
        id: &MeetingId,
        muted: bool,
    ) -> std::result::Result<(), SwitchDeviceError> {
        let m = self.get(id).await.ok_or(SwitchDeviceError::NotFound)?;

        let tx = m
            .audio_cmd_tx
            .lock()
            .await
            .clone()
            .ok_or(SwitchDeviceError::NotRecording)?;

        let (reply_tx, reply_rx) = oneshot::channel();
        tx.send(AudioCommand::SetMicMuted {
            muted,
            reply: reply_tx,
        })
        .await
        .map_err(|_| SwitchDeviceError::NotRecording)?;

        match tokio::time::timeout(std::time::Duration::from_secs(5), reply_rx).await {
            Ok(Ok(Ok(()))) => {
                *m.mic_muted.lock().await = muted;
                Ok(())
            }
            Ok(Ok(Err(msg))) => Err(SwitchDeviceError::Device(msg)),
            Ok(Err(_)) => Err(SwitchDeviceError::NotRecording),
            Err(_) => Err(SwitchDeviceError::Device(
                "timed out waiting for capture thread to set mic mute".into(),
            )),
        }
    }

    /// Open/close the mic echo on an actively-recording meeting.
    pub async fn set_echo(
        &self,
        id: &MeetingId,
        enabled: bool,
        device_name: String,
        buffer: u32,
    ) -> std::result::Result<(bool, String), SwitchDeviceError> {
        let m = self.get(id).await.ok_or(SwitchDeviceError::NotFound)?;

        let tx = m
            .audio_cmd_tx
            .lock()
            .await
            .clone()
            .ok_or(SwitchDeviceError::NotRecording)?;

        let (reply_tx, reply_rx) = oneshot::channel();
        tx.send(AudioCommand::SetEcho {
            enabled,
            device_name,
            buffer,
            reply: reply_tx,
        })
        .await
        .map_err(|_| SwitchDeviceError::NotRecording)?;

        match tokio::time::timeout(std::time::Duration::from_secs(5), reply_rx).await {
            Ok(Ok(Ok((live_enabled, device)))) => {
                *m.echo_enabled.lock().await = live_enabled;
                Ok((live_enabled, device))
            }
            Ok(Ok(Err(msg))) => {
                // open_echo tears down the previous stream first, so a failed open means echo is off.
                *m.echo_enabled.lock().await = false;
                Err(SwitchDeviceError::Device(msg))
            }
            Ok(Err(_)) => Err(SwitchDeviceError::NotRecording),
            Err(_) => Err(SwitchDeviceError::Device(
                "timed out waiting for capture thread to set echo".into(),
            )),
        }
    }
}

/// Continuation-session clock: elapsed wall-clock ms since this meeting's
/// ORIGINAL `started_at`, or `0` for a genuine first session.
///
/// Reads the meeting's row via `repo` — best-effort: no repo (Phase-3
/// in-memory-only callers), a missing row, or any DB/join error all mean
/// "can't tell, assume a first session" (offset 0), matching the safe
/// default a brand-new meeting should have anyway.
///
/// A row counts as a continuation exactly when `routes::start_stamp_patch`
/// would treat the upcoming `/start` as a restart: a set `ended_at` (the
/// meeting has been stopped at least once) paired with a nonzero
/// `started_at` (the schema's unstarted sentinel — see `V003__meetings.sql`
/// — is `0`; `MeetingPatch`-driven restarts never leave it there once a
/// real session has run). This function runs from inside `Registry::start`
/// BEFORE `routes::start_meeting`'s post-start patch executes, so at the
/// time this reads the row it still carries the *previous* session's
/// `started_at` — exactly the value the elapsed-time offset should be
/// measured from.
async fn session_offset_ms(repo: Option<&Arc<yogurt_db::MeetingRepo>>, meeting_id: &str) -> u64 {
    let Some(repo) = repo else {
        return 0;
    };
    let repo = repo.clone();
    let id = meeting_id.to_string();
    let row = match tokio::task::spawn_blocking(move || repo.get(&id)).await {
        Ok(Ok(Some(m))) => m,
        _ => return 0,
    };
    if row.started_at == 0 || row.ended_at.is_none() {
        return 0;
    }
    let now = now_ms() as i64;
    (now - row.started_at).max(0) as u64
}

/// Relay raw `TranscriptEvent`s from the STT engine onto the meeting's
/// public `transcript_tx`, adding `offset_ms` to every `ts_ms` along the
/// way. This is the ONE place a continuation session's clock gets shifted
/// forward, so restarting a meeting produces a monotonically increasing
/// transcript timeline instead of each session restarting at t=0 — which
/// would collide with (or precede) the prior session's timestamps once
/// both land in the same persisted `transcript_json` array.
///
/// Exits when the raw channel closes (the STT session ended and its sender
/// dropped) or lags — a lag here would silently skip segments, but the
/// channel's capacity (256) already gives ample headroom for real speech
/// cadence, mirroring the same tradeoff `pump_audio_adapter` makes for
/// audio frames.
async fn relay_transcript_events(
    mut raw_rx: broadcast::Receiver<TranscriptEvent>,
    tx: broadcast::Sender<TranscriptEvent>,
    offset_ms: u64,
) {
    let mut dedupe = EchoDeduper::default();
    loop {
        match raw_rx.recv().await {
            Ok(mut ev) => {
                ev.ts_ms += offset_ms;
                // ponytail: cross-channel TEXT dedupe, not echo cancellation.
                // When machine audio plays, SCK captures it digitally
                // (System) while the mic physically hears the speakers
                // (Mic), so the same speech transcribes twice. We suppress
                // the mic copy when it near-duplicates recent system text.
                // Known ceiling: mic partials pass through untouched (two
                // dim live lines during playback), and a mic final that
                // arrives before ANY covering system text (possible with
                // local whisper, which emits no system-channel partials)
                // survives. Upgrade path: proper AEC on the mic signal in
                // yogurt-audio (e.g. a webrtc/speex echo canceller fed the
                // system stream as its reference).
                if !dedupe.accept(&ev) {
                    tracing::debug!(
                        ts_ms = ev.ts_ms,
                        text = %ev.text,
                        "echo dedupe: suppressed mic segment duplicating system audio"
                    );
                    continue;
                }
                if is_noise(&ev) {
                    tracing::debug!(
                        ts_ms = ev.ts_ms,
                        text = %ev.text,
                        confidence = ?ev.confidence,
                        "noise filter: dropped segment"
                    );
                    continue;
                }
                if tx.send(ev).is_err() {
                    tracing::debug!("transcript relay: no receivers, terminating");
                    return;
                }
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!(n, "transcript relay lagged — events dropped");
            }
            Err(broadcast::error::RecvError::Closed) => return,
        }
    }
}

/// Wall-clock retention window for cross-channel echo dedupe: a mic-channel
/// final is checked against system-channel text seen within the last
/// `ECHO_DEDUPE_WINDOW_MS` of wall time. Arrival time rather than `ts_ms`
/// because `ts_ms` marks segment START and a single utterance can run 30 s+,
/// while the echo pair always ARRIVES within a few seconds of each other
/// (both engines hear the same audio at the same real moment).
const ECHO_DEDUPE_WINDOW_MS: u64 = 10_000;

/// Minimum bigram-containment score for a mic final to count as an echo of
/// recent system audio. Bigrams (adjacent word pairs) rather than single
/// words so a genuine mic utterance that merely re-uses vocabulary from the
/// machine audio is not swallowed.
const ECHO_DEDUPE_SIMILARITY: f64 = 0.8;

/// Mic finals with fewer normalized words than this skip dedupe entirely.
/// Short interjections ("yeah", "okay") legitimately occur on both channels
/// close together, and `[stt reconnecting]`-style status lines (which fire
/// on both channels at once) must stay visible.
const ECHO_DEDUPE_MIN_WORDS: usize = 3;

/// Backchannel/filler words dropped outright when they're ALL a segment
/// contains. Deliberately excludes "yeah", "okay", "right", "yes", "no" -
/// those are real one-word answers, and a word list can't tell backchannel
/// apart from an answer.
const FILLER_WORDS: &[&str] = &[
    "um", "umm", "uh", "uhh", "hmm", "hm", "mm", "mmm", "mhm", "mmhmm", "huh", "ah", "er", "erm",
    "oh", "eh",
];

/// Confidence floor below which a segment is dropped as noise. Conservative
/// so mumbled real speech survives; hallucinated words from coughs or taps
/// score well below it.
const MIN_CONFIDENCE: f32 = 0.4;

/// True when `ev` is backchannel filler or low-confidence noise and should
/// be dropped rather than relayed.
fn is_noise(ev: &TranscriptEvent) -> bool {
    if ev.confidence.is_some_and(|c| c < MIN_CONFIDENCE) {
        return true;
    }
    if is_sound_tag(&ev.text) {
        return true;
    }
    let words = normalize_words(&ev.text);
    !words.is_empty() && words.iter().all(|w| FILLER_WORDS.contains(&w.as_str()))
}

/// True when `text` is whisper's non-speech sound description rather than
/// transcribed speech: `*whistling*`, `[music]`, `(laughs)`, `♪ la la ♪`, or
/// an ALL-CAPS phrase like `BIRDS CHIRP`. Server status lines (`[stt ...]`)
/// are exempted since they share the bracket syntax.
fn is_sound_tag(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.starts_with("[stt ") {
        return false;
    }
    let is_wrapped = (trimmed.starts_with('*') && trimmed.ends_with('*'))
        || (trimmed.starts_with('[') && trimmed.ends_with(']'))
        || (trimmed.starts_with('(') && trimmed.ends_with(')'))
        || trimmed.contains('♪');
    if is_wrapped {
        return true;
    }
    // ponytail: an all-caps genuine answer ("OK") or acronym-only line is
    // dropped too; whisper normally casts real speech in sentence case.
    trimmed.chars().any(|c| c.is_alphabetic()) && !trimmed.chars().any(|c| c.is_lowercase())
}

/// Lowercase, split on every non-alphanumeric char, drop empties. Both
/// channels' text passes through the same normalization, so punctuation and
/// casing differences between the two STT sessions cancel out.
fn normalize_words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

/// Suppresses mic-channel finals that are near-duplicates of recent
/// system-channel text (acoustic echo: the mic hears the speakers while
/// ScreenCaptureKit captures the same audio digitally).
///
/// Direction is deliberately one-way: machine audio only ORIGINATES on the
/// system channel (SCK captures other apps' output; nothing plays the mic
/// back out), so the system copy is always the correctly-attributed one
/// ("Them") and only the mic copy is ever dropped. System events are never
/// suppressed; they are recorded as comparison material — finals within the
/// retention window plus the latest partial, so a mic final that beats the
/// matching system FINAL to arrival still matches the system partial text
/// already streamed.
#[derive(Default)]
struct EchoDeduper {
    /// (arrival, normalized words) of recent system finals, oldest first.
    sys_finals: VecDeque<(std::time::Instant, Vec<String>)>,
    /// Latest system partial — replaced in place on each partial, cleared
    /// when the covering final arrives.
    sys_partial: Option<(std::time::Instant, Vec<String>)>,
}

impl EchoDeduper {
    /// Returns `true` when the event should be forwarded.
    fn accept(&mut self, ev: &TranscriptEvent) -> bool {
        match ev.channel {
            Channel::System => {
                let words = normalize_words(&ev.text);
                let now = std::time::Instant::now();
                if ev.is_final {
                    self.sys_partial = None;
                    if !words.is_empty() {
                        self.sys_finals.push_back((now, words));
                    }
                } else if !words.is_empty() {
                    self.sys_partial = Some((now, words));
                }
                true
            }
            Channel::Mic => {
                if !ev.is_final {
                    return true;
                }
                self.evict();
                let words = normalize_words(&ev.text);
                if words.len() < ECHO_DEDUPE_MIN_WORDS {
                    return true;
                }
                self.system_bigram_containment(&words) < ECHO_DEDUPE_SIMILARITY
            }
        }
    }

    /// Drop retained system text older than the window.
    fn evict(&mut self) {
        let window = std::time::Duration::from_millis(ECHO_DEDUPE_WINDOW_MS);
        let now = std::time::Instant::now();
        while let Some((t, _)) = self.sys_finals.front() {
            if now.duration_since(*t) > window {
                self.sys_finals.pop_front();
            } else {
                break;
            }
        }
        if let Some((t, _)) = &self.sys_partial {
            if now.duration_since(*t) > window {
                self.sys_partial = None;
            }
        }
    }

    /// Fraction of `mic_words`' adjacent-pair bigrams present in the
    /// retained system text, counted as a multiset so a repeated phrase
    /// can't over-match. Pooling ALL retained entries (rather than
    /// comparing entry-by-entry) makes utterance-boundary mismatch a
    /// non-issue: one long system final vs. two shorter mic finals (or
    /// vice versa) still scores high.
    fn system_bigram_containment(&self, mic_words: &[String]) -> f64 {
        let mic_bigrams = mic_words.len().saturating_sub(1);
        if mic_bigrams == 0 {
            return 0.0;
        }
        let mut sys: HashMap<(&str, &str), usize> = HashMap::new();
        let entries = self
            .sys_finals
            .iter()
            .map(|(_, w)| w)
            .chain(self.sys_partial.iter().map(|(_, w)| w));
        for words in entries {
            for pair in words.windows(2) {
                *sys.entry((pair[0].as_str(), pair[1].as_str())).or_default() += 1;
            }
        }
        let mut hits = 0usize;
        for pair in mic_words.windows(2) {
            if let Some(n) = sys.get_mut(&(pair[0].as_str(), pair[1].as_str())) {
                if *n > 0 {
                    *n -= 1;
                    hits += 1;
                }
            }
        }
        hits as f64 / mic_bigrams as f64
    }
}

/// Best-effort broadcast of a fatal STT failure to the meeting's WS
/// subscribers. Before this, an STT session that died after `/start`
/// returned 200 was invisible: the user kept "recording" a meeting that
/// would never transcribe. The frontend surfaces `stt_error` as a banner.
fn send_stt_error(events_tx: &broadcast::Sender<serde_json::Value>, message: &str) {
    let _ = events_tx.send(serde_json::json!({
        "type": "stt_error",
        "message": message,
    }));
}

/// Stored-transcript segment shape — matches `yogurt_notes::TranscriptSegment`
/// and the `meetings.transcript_json` rows the enhance/chat prompts parse.
/// Mic audio is the user ("me"); system audio is everyone else ("them").
fn segment_json(ev: &TranscriptEvent) -> serde_json::Value {
    serde_json::json!({
        "ts_ms": ev.ts_ms,
        "channel": match ev.channel {
            Channel::Mic => "me",
            Channel::System => "them",
        },
        "text": ev.text,
    })
}

/// Accumulate final transcript segments into the meeting's SQLite row.
///
/// Writes on every final segment (SQLite-local, sub-ms; finals arrive well
/// under 1 Hz) so a crash mid-meeting loses at most the in-flight segment.
/// On shutdown: drain whatever is still queued in the broadcast, do one
/// last write, exit. Partials are never persisted — they're display-only.
///
/// Seeds its accumulator from the row's EXISTING `transcript_json` before
/// consuming any events — `write_segments` PATCHes the whole column, so a
/// continuation session (stop, then start again on the same meeting) that
/// started from an empty `Vec` silently destroyed the prior session's
/// transcript. Reading the seed happens after the broadcast subscription
/// this task's caller already set up, so no final can slip past while the
/// (bounded, sub-ms) seed read is in flight — the broadcast channel buffers
/// it regardless.
async fn persist_transcript(
    repo: Arc<yogurt_db::MeetingRepo>,
    meeting_id: String,
    mut rx: broadcast::Receiver<TranscriptEvent>,
    mut shutdown_rx: oneshot::Receiver<()>,
) {
    let mut segments: Vec<serde_json::Value> = load_existing_segments(&repo, &meeting_id).await;
    loop {
        tokio::select! {
            // Shutdown (or the sender being dropped by a failed start) both
            // mean: drain, flush, exit.
            _ = &mut shutdown_rx => break,
            res = rx.recv() => match res {
                Ok(ev) => {
                    if ev.is_final && !ev.text.trim().is_empty() {
                        segments.push(segment_json(&ev));
                        write_segments(&repo, &meeting_id, &segments).await;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(n, "transcript persistence lagged — segments dropped");
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    }
    // Drain anything still queued behind the shutdown signal.
    let mut drained_any = false;
    while let Ok(ev) = rx.try_recv() {
        if ev.is_final && !ev.text.trim().is_empty() {
            segments.push(segment_json(&ev));
            drained_any = true;
        }
    }
    if drained_any {
        write_segments(&repo, &meeting_id, &segments).await;
    }
}

/// Seed `persist_transcript`'s accumulator from the meeting's current
/// `transcript_json`. Never panics — a missing row, a DB/join error, or
/// malformed/empty JSON all fall back to an empty `Vec`, matching a
/// genuine first session (nothing to preserve).
async fn load_existing_segments(
    repo: &Arc<yogurt_db::MeetingRepo>,
    meeting_id: &str,
) -> Vec<serde_json::Value> {
    let repo = repo.clone();
    let id = meeting_id.to_string();
    tokio::task::spawn_blocking(move || repo.get(&id))
        .await
        .ok()
        .and_then(|r| r.ok())
        .flatten()
        .and_then(|m| serde_json::from_str::<Vec<serde_json::Value>>(&m.transcript_json).ok())
        .unwrap_or_default()
}

async fn write_segments(
    repo: &Arc<yogurt_db::MeetingRepo>,
    meeting_id: &str,
    segments: &[serde_json::Value],
) {
    let json = serde_json::Value::Array(segments.to_vec()).to_string();
    let repo = repo.clone();
    let id = meeting_id.to_string();
    let res = tokio::task::spawn_blocking(move || {
        repo.patch(
            &id,
            yogurt_db::MeetingPatch {
                transcript_json: Some(json),
                ..Default::default()
            },
        )
    })
    .await;
    match res {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => tracing::error!(error = %e, "transcript persistence write failed"),
        Err(e) => tracing::error!(error = %e, "transcript persistence join failed"),
    }
}

/// Service `AudioCommand`s (`SwitchMicDevice`, `SetMicMuted`) from the
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
    mut on_command: impl FnMut(AudioCommand),
) {
    rt_handle.block_on(async {
        loop {
            tokio::select! {
                _ = &mut shutdown_rx => break,
                cmd = cmd_rx.recv() => match cmd {
                    Some(cmd) => on_command(cmd),
                    None => break,
                }
            }
        }
    })
}

/// Throttled to at most one emission per channel per 100ms — feeds the
/// Granola-style live amplitude wave (browser `useAudioLevels`), not a
/// per-sample stream. `last_emit` is `None` until the first chunk on that
/// channel, so the very first level always fires immediately.
///
/// ponytail: wall-clock `Instant` throttle, not tokio's paused-time clock —
/// simplest thing that works for a per-chunk UI hint; upgrade only if a
/// test ever needs `tokio::time::pause()` semantics here.
fn maybe_emit_audio_level(
    events_tx: &broadcast::Sender<serde_json::Value>,
    last_emit: &mut Option<std::time::Instant>,
    channel: &str,
    peak: f32,
) {
    let now = std::time::Instant::now();
    let due = match last_emit {
        Some(t) => now.duration_since(*t) >= std::time::Duration::from_millis(100),
        None => true,
    };
    if !due {
        return;
    }
    *last_emit = Some(now);
    let level = (peak.clamp(0.0, 1.0) * 100.0).round() / 100.0;
    let _ = events_tx.send(serde_json::json!({
        "type": "audio_level",
        "channel": channel,
        "level": level,
    }));
}

/// Peak amplitude of a chunk, normalized 0..1 (`max(|sample|) / 32768`).
/// `unsigned_abs` sidesteps the `i16::MIN.abs()` overflow panic.
fn peak_amplitude(samples: &[i16]) -> f32 {
    samples.iter().map(|s| s.unsigned_abs()).max().unwrap_or(0) as f32 / 32768.0
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
/// Also emits throttled `audio_level` events on `events_tx` (see
/// `maybe_emit_audio_level`) so the browser can render a real amplitude
/// wave next to "Live transcript" instead of a heartbeat animation.
///
/// Extracted from the inline supervisor loop so the BL-04 behavior is unit-
/// testable without spinning up real SCK + cpal streams.
pub(crate) async fn pump_audio_adapter(
    mut mic_rx: broadcast::Receiver<yogurt_audio::Frame>,
    mut sys_rx: broadcast::Receiver<yogurt_audio::Frame>,
    audio_tx: broadcast::Sender<AudioChunk>,
    events_tx: broadcast::Sender<serde_json::Value>,
) {
    let mut mic_open = true;
    let mut sys_open = true;
    let mut mic_last_emit: Option<std::time::Instant> = None;
    let mut sys_last_emit: Option<std::time::Instant> = None;
    while mic_open || sys_open {
        tokio::select! {
            res = mic_rx.recv(), if mic_open => match res {
                Ok(frame) => {
                    maybe_emit_audio_level(&events_tx, &mut mic_last_emit, "mic", peak_amplitude(&frame.samples));
                    let chunk = AudioChunk {
                        channel: Channel::Mic,
                        samples: frame.samples,
                        ts_ms: frame.monotonic_micros / 1_000,
                    };
                    if audio_tx.send(chunk).is_err() {
                        tracing::debug!("audio adapter: no receivers, terminating");
                        return;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    // CLI-2: debug, not warn. This fires as a burst while the
                    // STT engine loads its model and the ring buffer catches
                    // up, which is every single meeting - a warning nobody can
                    // act on is noise. `RUST_LOG=yogurt=debug` brings it back.
                    // Revisit if lag ever turns sustained rather than a
                    // startup artifact: the fix then is a periodic count, not
                    // a line per event.
                    tracing::debug!(n, "audio adapter: mic lagged");
                }
                Err(broadcast::error::RecvError::Closed) => {
                    tracing::debug!("audio adapter: mic stream closed — continuing with system only");
                    mic_open = false;
                }
            },
            res = sys_rx.recv(), if sys_open => match res {
                Ok(frame) => {
                    maybe_emit_audio_level(&events_tx, &mut sys_last_emit, "system", peak_amplitude(&frame.samples));
                    let chunk = AudioChunk {
                        channel: Channel::System,
                        samples: frame.samples,
                        ts_ms: frame.monotonic_micros / 1_000,
                    };
                    if audio_tx.send(chunk).is_err() {
                        tracing::debug!("audio adapter: no receivers, terminating");
                        return;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::debug!(n, "audio adapter: system lagged");
                }
                Err(broadcast::error::RecvError::Closed) => {
                    tracing::debug!("audio adapter: system stream closed — continuing with mic only");
                    sys_open = false;
                }
            },
        }
    }
    tracing::debug!("audio adapter: both channels closed, terminating");
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
    async fn active_recording_is_none_when_nothing_started() {
        let reg = Registry::new();
        let _m = reg.create().await;
        assert_eq!(reg.active_recording().await, None);
    }

    #[tokio::test]
    async fn active_recording_finds_the_meeting_with_a_live_task() {
        let reg = Registry::new();
        let _idle = reg.create().await;
        let recording = reg.create().await;
        *recording.task.lock().await = Some(tokio::spawn(async {}));
        assert_eq!(reg.active_recording().await, Some(recording.id));
    }

    /// Single-active-recording invariant: starting meeting B while meeting
    /// A records must fail with an actionable error - and BEFORE any
    /// settings validation or audio capture is attempted.
    #[tokio::test]
    async fn it_refuses_to_start_a_second_recording() {
        let reg = Registry::new();
        let a = reg.create().await;
        *a.task.lock().await = Some(tokio::spawn(async {
            std::future::pending::<()>().await;
        }));
        let b = reg.create().await;

        let err = reg
            .start(&b.id, SttSettings::default(), None, None)
            .await
            .expect_err("second concurrent recording must be refused");
        assert!(
            format!("{err:#}").contains("another meeting is already recording"),
            "got: {err:#}"
        );

        // Restarting the SAME already-live meeting stays its own error.
        let err = reg
            .start(&a.id, SttSettings::default(), None, None)
            .await
            .expect_err("double start of the live meeting must be refused");
        assert!(
            format!("{err:#}").contains("already started"),
            "got: {err:#}"
        );

        let leftover = a.task.lock().await.take();
        if let Some(t) = leftover {
            t.abort();
        }
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
                confidence: None,
            })
            .unwrap();

        let a = rx1.recv().await.unwrap();
        let b = rx2.recv().await.unwrap();
        assert_eq!(a.text, "hi");
        assert_eq!(b.text, "hi");
    }

    /// Proves the real `tokio::select!` capture-thread control loop
    /// services both `SwitchMicDevice` (AUD-3) and `SetMicMuted` (AUD-6)
    /// commands, interleaved, in order, and exits cleanly on shutdown, with
    /// no deadlock — entirely independent of real audio hardware (mirrors
    /// how `pump_audio_adapter` is tested). A `1s` timeout on each reply
    /// means a deadlock fails the test loudly rather than hanging the test
    /// run.
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
        // captured `Handle`, with one dispatch closure servicing every
        // command variant (see the real call site's comment for why this
        // can't be one closure per variant).
        let worker = std::thread::spawn(move || {
            run_capture_control_loop(&handle, shutdown_rx, cmd_rx, move |cmd| match cmd {
                AudioCommand::SwitchMicDevice { device_name, reply } => {
                    seen_for_worker
                        .lock()
                        .unwrap()
                        .push(format!("switch:{device_name}"));
                    let _ = reply.send(Ok((format!("resolved:{device_name}"), false)));
                }
                AudioCommand::SetMicMuted { muted, reply } => {
                    seen_for_worker
                        .lock()
                        .unwrap()
                        .push(format!("mute:{muted}"));
                    let _ = reply.send(Ok(()));
                }
                AudioCommand::SetEcho { enabled, reply, .. } => {
                    seen_for_worker
                        .lock()
                        .unwrap()
                        .push(format!("echo:{enabled}"));
                    let _ = reply.send(Ok((enabled, "test-device".into())));
                }
            });
        });

        rt.block_on(async {
            for name in ["mic-a", "mic-b"] {
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
                assert_eq!(reply, Ok((format!("resolved:{name}"), false)));
            }

            for muted in [true, false] {
                let (reply_tx, reply_rx) = oneshot::channel();
                cmd_tx
                    .send(AudioCommand::SetMicMuted {
                        muted,
                        reply: reply_tx,
                    })
                    .await
                    .expect("send command");
                let reply = tokio::time::timeout(std::time::Duration::from_secs(1), reply_rx)
                    .await
                    .expect("reply within 1s — a timeout means the select loop deadlocked")
                    .expect("reply sender dropped unexpectedly");
                assert_eq!(reply, Ok(()));
            }
        });

        // Signal shutdown and join — a hang here means the loop can't
        // observe the dropped oneshot and exit cleanly.
        drop(shutdown_tx);
        worker.join().expect("worker thread joins cleanly");

        assert_eq!(
            *seen.lock().unwrap(),
            vec![
                "switch:mic-a".to_string(),
                "switch:mic-b".to_string(),
                "mute:true".to_string(),
                "mute:false".to_string(),
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
        let (events_tx, _events_rx) = broadcast::channel::<serde_json::Value>(16);

        // Drop the system sender immediately → sys_rx will see Closed on
        // its first recv.
        drop(sys_tx);

        let pump = tokio::spawn(pump_audio_adapter(mic_rx, sys_rx, audio_tx, events_tx));

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
        let (events_tx, _events_rx) = broadcast::channel::<serde_json::Value>(16);

        drop(mic_tx);

        let pump = tokio::spawn(pump_audio_adapter(mic_rx, sys_rx, audio_tx, events_tx));
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
        let (events_tx, _events_rx) = broadcast::channel::<serde_json::Value>(16);

        drop(mic_tx);
        drop(sys_tx);

        let pump = tokio::spawn(pump_audio_adapter(mic_rx, sys_rx, audio_tx, events_tx));
        tokio::time::timeout(std::time::Duration::from_secs(1), pump)
            .await
            .expect("pump must exit promptly when both channels closed")
            .expect("pump task joined");
    }

    /// Feeds a burst of mic frames with a known peak amplitude, asserts the
    /// resulting `audio_level` event reports the right level and channel,
    /// and that the 100ms throttle collapses a same-instant burst down to
    /// far fewer events than frames sent. Real wall-clock sleeps (not
    /// `tokio::time::pause`) since `maybe_emit_audio_level` throttles on
    /// `std::time::Instant`, not tokio's virtual clock.
    #[tokio::test]
    async fn pump_emits_throttled_audio_level_events() {
        let (mic_tx, mic_rx) = broadcast::channel::<yogurt_audio::Frame>(64);
        let (sys_tx, sys_rx) = broadcast::channel::<yogurt_audio::Frame>(16);
        let (audio_tx, _audio_rx) = broadcast::channel::<AudioChunk>(64);
        let (events_tx, mut events_rx) = broadcast::channel::<serde_json::Value>(64);

        let pump = tokio::spawn(pump_audio_adapter(mic_rx, sys_rx, audio_tx, events_tx));

        // Half-scale mic frame (samples peak at 16384 -> level ~0.5).
        let half_scale = vec![16384i16; 320];
        mic_tx
            .send(yogurt_audio::Frame {
                channel: yogurt_audio::Channel::Mic,
                samples: half_scale.clone(),
                monotonic_micros: 0,
            })
            .unwrap();
        let first = tokio::time::timeout(std::time::Duration::from_millis(500), events_rx.recv())
            .await
            .expect("first audio_level within 500ms")
            .expect("recv ok");
        assert_eq!(first["type"], "audio_level");
        assert_eq!(first["channel"], "mic");
        let level = first["level"].as_f64().expect("level is a number");
        assert!((level - 0.5).abs() < 0.01, "expected ~0.5, got {level}");

        // Burst 9 more frames immediately (well within the 100ms throttle
        // window) — none of these should produce a second event yet.
        for i in 1..10u64 {
            mic_tx
                .send(yogurt_audio::Frame {
                    channel: yogurt_audio::Channel::Mic,
                    samples: half_scale.clone(),
                    monotonic_micros: i * 20_000,
                })
                .unwrap();
        }
        let burst_result =
            tokio::time::timeout(std::time::Duration::from_millis(60), events_rx.recv()).await;
        assert!(
            burst_result.is_err(),
            "burst within the throttle window must not emit a second event, got {burst_result:?}"
        );

        // After the throttle window elapses (real time), the next frame
        // must emit a fresh event.
        tokio::time::sleep(std::time::Duration::from_millis(110)).await;
        let full_scale = vec![i16::MIN; 320];
        mic_tx
            .send(yogurt_audio::Frame {
                channel: yogurt_audio::Channel::Mic,
                samples: full_scale,
                monotonic_micros: 300_000,
            })
            .unwrap();
        let second = tokio::time::timeout(std::time::Duration::from_millis(500), events_rx.recv())
            .await
            .expect("second audio_level within 500ms")
            .expect("recv ok");
        assert_eq!(second["channel"], "mic");
        let level2 = second["level"].as_f64().expect("level is a number");
        assert!((level2 - 1.0).abs() < 0.01, "expected ~1.0, got {level2}");

        drop(mic_tx);
        drop(sys_tx);
        tokio::time::timeout(std::time::Duration::from_secs(1), pump)
            .await
            .expect("pump should exit when both channels closed")
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
        let msg = format!("{err:#}").to_lowercase();
        assert!(msg.contains("api key"), "got: {msg}");
        // The fix path must point at Settings, not at a raw env var.
        assert!(msg.contains("settings"), "actionable msg: {msg}");
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

    // ─── `persist_transcript` regression coverage ───────────────────────

    /// In-memory `MeetingRepo` fixture shared by the `persist_transcript`
    /// tests below. Mirrors `yogurt_db::meetings::tests::fresh_repo` but
    /// lives here since `meetings.rs` doesn't depend on that test helper.
    fn fresh_repo_with_row() -> (Arc<yogurt_db::MeetingRepo>, String) {
        let db = yogurt_db::Db::open_in_memory().expect("open in-memory db");
        let repo = Arc::new(yogurt_db::MeetingRepo::new(db));
        let m = repo
            .create(yogurt_db::NewMeeting {
                title: "Test meeting".into(),
                ..Default::default()
            })
            .expect("create meeting row");
        (repo, m.id)
    }

    fn ev(ts_ms: u64, channel: Channel, text: &str, is_final: bool) -> TranscriptEvent {
        TranscriptEvent {
            ts_ms,
            channel,
            text: text.to_string(),
            is_final,
            confidence: None,
        }
    }

    fn ev_conf(text: &str, confidence: Option<f32>) -> TranscriptEvent {
        TranscriptEvent {
            ts_ms: 0,
            channel: Channel::Mic,
            text: text.to_string(),
            is_final: true,
            confidence,
        }
    }

    /// Clear duplicate: the mic hears exactly what the speakers played.
    /// The system final passes; the identical mic final is suppressed.
    #[test]
    fn echo_dedupe_suppresses_exact_mic_duplicate_of_system_final() {
        let mut d = EchoDeduper::default();
        let text = "welcome back to the channel twenty months ago I got a job at Disney";
        assert!(d.accept(&ev(100, Channel::System, text, true)));
        assert!(
            !d.accept(&ev(150, Channel::Mic, text, true)),
            "identical mic final must be suppressed as echo"
        );
    }

    /// Near-duplicate with minor STT variation (different punctuation,
    /// casing, and one small word swap between the two engine sessions)
    /// is still recognized as echo.
    #[test]
    fn echo_dedupe_suppresses_mic_near_duplicate_with_stt_variation() {
        let mut d = EchoDeduper::default();
        assert!(d.accept(&ev(
            100,
            Channel::System,
            "more about why i quit and a tour of my new apartment later in this video",
            true,
        )));
        assert!(
            !d.accept(&ev(
                150,
                Channel::Mic,
                "More about why I quit, and the tour of my new apartment later in this video.",
                true,
            )),
            "near-duplicate mic final must be suppressed as echo"
        );
    }

    /// Distinct speech in the same window must NOT be deduped: the user
    /// talking over machine audio is genuine mic content.
    #[test]
    fn echo_dedupe_keeps_distinct_mic_speech_in_same_window() {
        let mut d = EchoDeduper::default();
        assert!(d.accept(&ev(
            100,
            Channel::System,
            "the quarterly numbers look strong across every region this year",
            true,
        )));
        assert!(
            d.accept(&ev(
                150,
                Channel::Mic,
                "can you send me the slides after this meeting wraps up",
                true,
            )),
            "distinct mic speech must pass through"
        );
    }

    /// Short interjections are exempt (ECHO_DEDUPE_MIN_WORDS): "yeah" from
    /// the user right after "yeah" from the machine audio is plausible
    /// genuine speech, not necessarily echo.
    #[test]
    fn echo_dedupe_keeps_short_mic_interjections() {
        let mut d = EchoDeduper::default();
        assert!(d.accept(&ev(100, Channel::System, "yeah okay", true)));
        assert!(
            d.accept(&ev(150, Channel::Mic, "yeah okay", true)),
            "short mic finals skip dedupe entirely"
        );
    }

    /// The mic final may arrive BEFORE the matching system final (per-channel
    /// STT sessions finalize independently). The system channel's already-
    /// streamed PARTIAL text must serve as comparison material.
    #[test]
    fn echo_dedupe_matches_against_latest_system_partial() {
        let mut d = EchoDeduper::default();
        assert!(d.accept(&ev(
            100,
            Channel::System,
            "i put in my two weeks last monday and it's honestly a pretty crazy feeling",
            false,
        )));
        assert!(
            !d.accept(&ev(
                150,
                Channel::Mic,
                "I put in my two weeks last Monday and it's honestly a pretty crazy feeling.",
                true,
            )),
            "mic final duplicating the live system partial must be suppressed"
        );
    }

    /// System text older than ECHO_DEDUPE_WINDOW_MS is evicted — a matching
    /// mic final long after the machine audio stopped is genuine speech
    /// (e.g. the user reading the same sentence aloud a minute later).
    #[test]
    fn echo_dedupe_forgets_system_text_outside_window() {
        let mut d = EchoDeduper::default();
        let text = "twenty months ago i got a job as a software engineer at disney";
        let stale = std::time::Instant::now()
            .checked_sub(std::time::Duration::from_millis(
                ECHO_DEDUPE_WINDOW_MS + 1_000,
            ))
            .expect("host uptime exceeds the dedupe window");
        d.sys_finals.push_back((stale, normalize_words(text)));
        assert!(
            d.accept(&ev(60_000, Channel::Mic, text, true)),
            "system text outside the retention window must not suppress mic speech"
        );
    }

    /// Mic partials are never suppressed (ponytail ceiling: live partial
    /// lines during playback are accepted; AEC is the real fix), and system
    /// events are never suppressed in either direction.
    #[test]
    fn echo_dedupe_never_suppresses_partials_or_system_events() {
        let mut d = EchoDeduper::default();
        let text = "for now welcome back to the channel and thanks for watching";
        assert!(d.accept(&ev(100, Channel::System, text, true)));
        assert!(d.accept(&ev(150, Channel::Mic, text, false)), "mic partial");
        assert!(
            d.accept(&ev(200, Channel::System, text, true)),
            "system final repeating earlier system text"
        );
    }

    #[test]
    fn is_noise_drops_pure_filler() {
        assert!(is_noise(&ev_conf("Um, uh-huh.", None)));
    }

    #[test]
    fn is_noise_keeps_filler_mixed_with_real_words() {
        assert!(!is_noise(&ev_conf("um so I think", None)));
    }

    #[test]
    fn is_noise_drops_low_confidence() {
        assert!(is_noise(&ev_conf("hello world", Some(0.1))));
    }

    #[test]
    fn is_noise_keeps_confidence_at_threshold() {
        assert!(!is_noise(&ev_conf("hello world", Some(MIN_CONFIDENCE))));
    }

    #[test]
    fn is_noise_keeps_none_confidence() {
        assert!(!is_noise(&ev_conf("hello world", None)));
    }

    #[test]
    fn is_noise_keeps_empty_text() {
        assert!(!is_noise(&ev_conf("", None)));
    }

    #[test]
    fn is_noise_drops_sound_tags() {
        assert!(is_noise(&ev_conf("*whistling*", None)));
        assert!(is_noise(&ev_conf("[music]", None)));
        assert!(is_noise(&ev_conf("(laughs)", None)));
        assert!(is_noise(&ev_conf("♪ la la ♪", None)));
        assert!(is_noise(&ev_conf("BIRDS CHIRP", None)));
    }

    #[test]
    fn is_noise_keeps_stt_status_lines() {
        assert!(!is_noise(&ev_conf("[stt reconnecting]", None)));
        assert!(!is_noise(&ev_conf(
            "[stt overloaded, transcript may be lossy]",
            None
        )));
    }

    #[test]
    fn is_noise_keeps_real_speech() {
        assert!(!is_noise(&ev_conf("Okay, are we ready to talk?", None)));
        assert!(!is_noise(&ev_conf(
            "I'm gonna go make my food first.",
            None
        )));
        assert!(!is_noise(&ev_conf("*whistling* then we talked", None)));
    }

    /// Drives `persist_transcript` directly against a broadcast channel: a
    /// mix of finals/partials/empty-text-finals on both channels, then a
    /// shutdown signal. Asserts the row's `transcript_json` ends up with
    /// exactly the non-empty finals, in arrival order, with the correct
    /// me/them channel mapping (mic -> "me", system -> "them").
    #[tokio::test]
    async fn persist_transcript_accumulates_only_nonempty_finals_in_order() {
        let (repo, meeting_id) = fresh_repo_with_row();
        let (tx, rx) = broadcast::channel::<TranscriptEvent>(16);
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

        let handle = tokio::spawn(persist_transcript(
            repo.clone(),
            meeting_id.clone(),
            rx,
            shutdown_rx,
        ));

        // Mix: a mic final (kept), a mic partial (ignored — not final), a
        // system final with empty/whitespace text (ignored), and a system
        // final with real text (kept).
        tx.send(ev(100, Channel::Mic, "hello", true)).unwrap();
        tx.send(ev(150, Channel::Mic, "in progress", false))
            .unwrap();
        tx.send(ev(200, Channel::System, "   ", true)).unwrap();
        tx.send(ev(250, Channel::System, "world", true)).unwrap();

        // Give the task a beat to process the per-final writes before we
        // signal shutdown, so this exercises the steady-state write path
        // (not just the drain-on-shutdown path covered by the next test).
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let _ = shutdown_tx.send(());
        handle.await.expect("persist task joins cleanly");

        let row = repo.get(&meeting_id).expect("get row").expect("row exists");
        let segments: Vec<serde_json::Value> =
            serde_json::from_str(&row.transcript_json).expect("valid JSON");
        assert_eq!(
            segments.len(),
            2,
            "only the two non-empty finals should persist; got {segments:?}"
        );
        assert_eq!(segments[0]["ts_ms"], 100);
        assert_eq!(segments[0]["channel"], "me");
        assert_eq!(segments[0]["text"], "hello");
        assert_eq!(segments[1]["ts_ms"], 250);
        assert_eq!(segments[1]["channel"], "them");
        assert_eq!(segments[1]["text"], "world");
    }

    /// THE data-loss bug: `write_segments` PATCHes the whole
    /// `transcript_json` column, so a second `persist_transcript` session
    /// on the SAME meeting row must seed its accumulator from what's
    /// already there — otherwise session 2's first write truncates session
    /// 1's finals. Runs two full session lifecycles (spawn, send finals,
    /// shutdown, join) against the same repo row and asserts the row ends
    /// up with BOTH sessions' finals, in order.
    #[tokio::test]
    async fn persist_transcript_accumulates_across_sequential_sessions() {
        let (repo, meeting_id) = fresh_repo_with_row();

        // Session 1.
        let (tx1, rx1) = broadcast::channel::<TranscriptEvent>(16);
        let (shutdown_tx1, shutdown_rx1) = oneshot::channel::<()>();
        let handle1 = tokio::spawn(persist_transcript(
            repo.clone(),
            meeting_id.clone(),
            rx1,
            shutdown_rx1,
        ));
        tx1.send(ev(100, Channel::Mic, "hello", true)).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let _ = shutdown_tx1.send(());
        handle1.await.expect("session 1 persist task joins cleanly");

        // Session 2 — fresh broadcast channel (mirrors a real restart:
        // `Registry::start` mints a new one), SAME repo row.
        let (tx2, rx2) = broadcast::channel::<TranscriptEvent>(16);
        let (shutdown_tx2, shutdown_rx2) = oneshot::channel::<()>();
        let handle2 = tokio::spawn(persist_transcript(
            repo.clone(),
            meeting_id.clone(),
            rx2,
            shutdown_rx2,
        ));
        tx2.send(ev(500, Channel::System, "world", true)).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let _ = shutdown_tx2.send(());
        handle2.await.expect("session 2 persist task joins cleanly");

        let row = repo.get(&meeting_id).expect("get row").expect("row exists");
        let segments: Vec<serde_json::Value> =
            serde_json::from_str(&row.transcript_json).expect("valid JSON");
        assert_eq!(
            segments.len(),
            2,
            "both sessions' finals must survive; got {segments:?}"
        );
        assert_eq!(segments[0]["text"], "hello", "session 1's final first");
        assert_eq!(segments[1]["text"], "world", "session 2's final appended");
    }

    /// Malformed / empty existing `transcript_json` must seed an empty
    /// `Vec` — never panic. Covers a corrupt row (hand-edited DB, a prior
    /// bug) so a bad seed can't crash the persistence task.
    #[tokio::test]
    async fn persist_transcript_seeds_empty_on_malformed_existing_json() {
        let (repo, meeting_id) = fresh_repo_with_row();
        repo.patch(
            &meeting_id,
            yogurt_db::MeetingPatch {
                transcript_json: Some("not valid json".into()),
                ..Default::default()
            },
        )
        .expect("seed malformed transcript_json");

        let (tx, rx) = broadcast::channel::<TranscriptEvent>(16);
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let handle = tokio::spawn(persist_transcript(
            repo.clone(),
            meeting_id.clone(),
            rx,
            shutdown_rx,
        ));
        tx.send(ev(10, Channel::Mic, "fresh start", true)).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let _ = shutdown_tx.send(());
        handle
            .await
            .expect("persist task joins cleanly despite bad seed");

        let row = repo.get(&meeting_id).expect("get row").expect("row exists");
        let segments: Vec<serde_json::Value> =
            serde_json::from_str(&row.transcript_json).expect("valid JSON");
        assert_eq!(
            segments.len(),
            1,
            "malformed seed must not panic or leak junk"
        );
        assert_eq!(segments[0]["text"], "fresh start");
    }

    // ─── `session_offset_ms` regression coverage ─────────────────────────

    /// No repo at all (Phase-3 in-memory-only caller) — must default to 0
    /// rather than panic or block forever.
    #[tokio::test]
    async fn session_offset_ms_is_zero_without_a_repo() {
        assert_eq!(session_offset_ms(None, "whatever").await, 0);
    }

    /// A genuine first session: fresh row, unstarted sentinel, no
    /// `ended_at`. Offset must be 0 — there is no prior session to
    /// continue from.
    #[tokio::test]
    async fn session_offset_ms_is_zero_for_a_fresh_meeting() {
        let (repo, meeting_id) = fresh_repo_with_row();
        assert_eq!(session_offset_ms(Some(&repo), &meeting_id).await, 0);
    }

    /// A meeting that has been started but never stopped (`ended_at` still
    /// `None`) is NOT a continuation by this function's contract — offset
    /// stays 0. (Whether `Registry::start` can even reach this state is a
    /// separate question; the function's job is just to read the row.)
    #[tokio::test]
    async fn session_offset_ms_is_zero_when_never_stopped() {
        let (repo, meeting_id) = fresh_repo_with_row();
        repo.patch(
            &meeting_id,
            yogurt_db::MeetingPatch {
                started_at: Some(1_000),
                ..Default::default()
            },
        )
        .expect("stamp started_at");
        assert_eq!(session_offset_ms(Some(&repo), &meeting_id).await, 0);
    }

    /// A continuation session: `started_at` set from a real prior session
    /// and `ended_at` set from the stop that followed it. Offset must be
    /// the elapsed wall-clock ms since that `started_at` — asserted as a
    /// tight window around `now - started_at` rather than an exact value,
    /// since the function reads a real wall clock.
    #[tokio::test]
    async fn session_offset_ms_is_elapsed_time_for_a_continuation_session() {
        let (repo, meeting_id) = fresh_repo_with_row();
        let started_at = now_ms() as i64 - 60_000; // pretend session 1 started 60s ago
        repo.patch(
            &meeting_id,
            yogurt_db::MeetingPatch {
                started_at: Some(started_at),
                ended_at: Some(Some(started_at + 30_000)), // stopped 30s in
                ..Default::default()
            },
        )
        .expect("stamp session-1-completed state");

        let offset = session_offset_ms(Some(&repo), &meeting_id).await;
        assert!(
            (59_000..61_000).contains(&offset),
            "offset should be ~60000ms elapsed since started_at, got {offset}"
        );
    }

    /// Events sent into the broadcast channel BEFORE the task ever polls
    /// it (and shutdown signaled immediately after) must still be drained
    /// and persisted — the "drain whatever is still queued" tail of
    /// `persist_transcript` must not depend on the per-final write path
    /// having already run.
    #[tokio::test]
    async fn persist_transcript_drains_unconsumed_events_on_immediate_shutdown() {
        let (repo, meeting_id) = fresh_repo_with_row();
        let (tx, rx) = broadcast::channel::<TranscriptEvent>(16);
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

        // Queue several finals on the broadcast buffer, then signal
        // shutdown immediately — none of this has been consumed by any
        // task yet (the persist task hasn't even been spawned).
        for i in 0..3u64 {
            tx.send(ev(i * 10, Channel::Mic, &format!("seg{i}"), true))
                .unwrap();
        }
        let _ = shutdown_tx.send(());

        let handle = tokio::spawn(persist_transcript(
            repo.clone(),
            meeting_id.clone(),
            rx,
            shutdown_rx,
        ));
        handle.await.expect("persist task joins cleanly");

        let row = repo.get(&meeting_id).expect("get row").expect("row exists");
        let segments: Vec<serde_json::Value> =
            serde_json::from_str(&row.transcript_json).expect("valid JSON");
        assert_eq!(
            segments.len(),
            3,
            "all pre-queued finals must be drained and persisted; got {segments:?}"
        );
        for (i, seg) in segments.iter().enumerate() {
            assert_eq!(seg["text"], format!("seg{i}"));
        }
    }
}
