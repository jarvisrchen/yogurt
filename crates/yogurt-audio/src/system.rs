//! macOS system-audio loopback capture via `screencapturekit` 8.x
//! (Path A from the Plan 02-01 spike: PASS — in-process SCK is sufficient
//! for v1, the Swift sidecar fallback (Path B) is not needed).
//!
//! ## Pipeline (BL-01 / BL-03 architecture)
//!
//! ```text
//! SCK dispatch thread:                tokio drainer task:
//!   audio_buffer_list() → f32           drain SPSC ring
//!   channel-mean → mono                 rubato 48k → 16k
//!   push to SPSC ring (lock-free)       f32 → i16 (clamp + multiply)
//!         │                              FrameChunker → broadcast::Sender
//!         └─────► SPSC ring ─────►
//! ```
//!
//! The SCK callback NEVER takes a mutex, NEVER allocates beyond a small
//! stack scratch buffer, and NEVER sends on the broadcast channel. Per
//! SCK's API quirk #2, callbacks may fire concurrently from arbitrary
//! dispatch queues — that's safe here because each callback only writes
//! to the SPSC ring (single-producer model relies on each callback's
//! own queue serialization plus SCK's contract that audio frames are
//! issued in order). The drainer is the sole consumer.
//!
//! ## Critical configuration (AUDIO-03)
//!
//! `with_excludes_current_process_audio(true)` is set from the first commit.
//! Without it, yogurt's own UI sounds (or any audio the binary plays back)
//! would loop back into the system capture stream, contaminating the
//! transcript. This is **load-bearing for privacy and correctness**.
//!
//! ## API quirks (from the Plan 02-01 spike note)
//!
//! 1. Audio-only `SCStream` still needs a valid video config: `width ≥ 2`,
//!    `height ≥ 2`. Omitting them produces an opaque error at
//!    `start_capture()`.
//! 2. Each callback's `audio_buffer_list()` yields **two parallel mono
//!    buffers** (L, R), not one interleaved buffer. The handler iterates
//!    the parallel channels and averages sample-by-sample.
//! 3. Handlers must be `Fn + Send + Sync + 'static`. The new SPSC-ring
//!    architecture satisfies this without a mutex (the producer half is
//!    moved into the handler closure; SCK never invokes two callbacks
//!    concurrently for the same output type per its dispatch contract).
//!
//! ## Watchdog (AUD-14)
//!
//! SCK can stop delivering mid-meeting (display change, sleep, a Zoom
//! window going away) while the mic keeps working, and the broadcast
//! sender stays open, so downstream only ever sees silence. A supervisor
//! task rebuilds the `SCStream` in place when the delegate reports a stop
//! or no buffers arrive for [`macos::STALL`]. The ring, drainer and
//! broadcast sender are reused, so subscribers never notice the swap.
//!
//! Measured on macOS 26 with nothing playing: SCK still delivers all-zero
//! buffers (207 frames and peak 0 in 5 s via `examples/system_smoke.rs`),
//! so seconds without a buffer mean the stream is dead, not that the room
//! is quiet.

use crate::{
    error::{AudioError, Result},
    frame::{Channel, Frame},
    mic::FrameChunker,
};
use tokio::sync::broadcast;

/// Health of the system-audio stream, relayed to the meeting's WS clients.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemStatus {
    /// No audio arrived for several seconds (or SCK reported a stop). A restart is underway.
    Stalled,
    /// Audio is flowing again after a restart.
    Recovered,
    /// Restarts kept failing. Capture will not be retried again.
    Failed,
}

/// Live system-audio capture handle. Dropping it stops the underlying
/// `SCStream` (BL-04: explicit `stop_capture()` + drain wait) and aborts
/// the drainer task.
pub struct SystemCapture {
    status_tx: broadcast::Sender<SystemStatus>,
    #[cfg(target_os = "macos")]
    _inner: macos::Inner,
}

impl std::fmt::Debug for SystemCapture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SystemCapture").finish_non_exhaustive()
    }
}

impl SystemCapture {
    /// Subscribe to stall / recovery / failure notices for this capture.
    pub fn subscribe_status(&self) -> broadcast::Receiver<SystemStatus> {
        self.status_tx.subscribe()
    }
}

/// Spawn a system-audio loopback capture. Returns a [`SystemCapture`] whose
/// `Drop` impl stops the underlying SCK stream.
///
/// **Permission gate:** on macOS, returns [`AudioError::PermissionDenied`]
/// immediately if Screen Recording is not granted — the user never hears
/// a recording indicator if capture is going to fail (D-25). On non-macOS,
/// returns [`AudioError::UnsupportedPlatform`].
#[cfg(target_os = "macos")]
pub fn spawn_system_capture(tx: broadcast::Sender<Frame>) -> Result<SystemCapture> {
    let (status_tx, _) = broadcast::channel(8);
    let inner = macos::start(tx, status_tx.clone())?;
    Ok(SystemCapture {
        status_tx,
        _inner: inner,
    })
}

/// Non-macOS stub. System audio loopback requires `ScreenCaptureKit`.
#[cfg(not(target_os = "macos"))]
pub fn spawn_system_capture(_tx: broadcast::Sender<Frame>) -> Result<SystemCapture> {
    Err(AudioError::UnsupportedPlatform)
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use crate::permission::{has_screen_recording_permission, PermissionStatus};
    use crate::resample::Downmix;
    use crate::ring::{
        ring, AudioRingConsumer, AudioRingProducer, DRAINER_SCRATCH_HINT, DRAINER_TICK,
    };
    use parking_lot::Mutex;
    use screencapturekit::prelude::*;
    use std::future::Future;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::Notify;
    use tokio::task::JoinHandle;
    use tokio::time::Instant;

    /// SCK output is fixed-format 48 kHz f32 stereo per the spike's
    /// empirical observation (and matches the `AudioSampleRate` /
    /// `AudioChannelCount` defaults).
    const SCK_INPUT_RATE: u32 = 48_000;
    const SCK_INPUT_CHANNELS: u16 = 2;

    /// BL-04: post-stop_capture() drain delay. SCK's dispatch queue may
    /// have one or more in-flight `did_output_sample_buffer` invocations
    /// that landed before stop took effect; waiting ~50 ms lets them
    /// complete before we drop the SPSC ring producer and the drainer.
    const STOP_CAPTURE_DRAIN_WAIT: Duration = Duration::from_millis(50);

    /// No SCK buffers for this long means the stream is dead (see the module docs).
    pub(super) const STALL: Duration = Duration::from_secs(5);
    const WATCHDOG_TICK: Duration = Duration::from_secs(1);
    /// Restart attempt `n` (from 0) waits `min(BACKOFF_BASE * 2^(n-1), BACKOFF_CAP)`; the first is immediate.
    const BACKOFF_BASE: Duration = Duration::from_secs(1);
    const BACKOFF_CAP: Duration = Duration::from_secs(30);
    /// Consecutive restarts that bring no audio back before giving up.
    const MAX_ATTEMPTS: u32 = 6;
    const SUMMARY_EVERY: Duration = Duration::from_secs(30);

    /// Counters shared by the SCK callback, the drainer and the watchdog.
    #[derive(Debug, Default)]
    pub(super) struct SysStats {
        /// Audio callbacks SCK has delivered. The watchdog's heartbeat.
        pub(super) callbacks: AtomicU64,
        pub(super) restarts: AtomicU64,
    }

    /// Something the watchdog can rebuild.
    pub(super) trait Restart: Send + 'static {
        fn restart(&mut self) -> impl Future<Output = Result<()>> + Send;
    }

    /// Rebuild `src` whenever `stopped` fires or `stats.callbacks` stops moving
    /// for [`STALL`], with exponential backoff, until audio returns or
    /// [`MAX_ATTEMPTS`] restarts in a row fail to bring it back.
    pub(super) async fn supervise<S: Restart>(
        mut src: S,
        stats: Arc<SysStats>,
        stopped: Arc<Notify>,
        status_tx: broadcast::Sender<SystemStatus>,
    ) {
        let mut seen = stats.callbacks.load(Ordering::Relaxed);
        let mut last_progress = Instant::now();
        let mut attempts = 0u32;
        let mut tick = tokio::time::interval(WATCHDOG_TICK);
        loop {
            let delegate_stop = tokio::select! {
                _ = tick.tick() => false,
                _ = stopped.notified() => true,
            };
            let now = stats.callbacks.load(Ordering::Relaxed);
            if now != seen {
                seen = now;
                last_progress = Instant::now();
                if attempts > 0 {
                    tracing::info!(attempts, "system audio recovered");
                    let _ = status_tx.send(SystemStatus::Recovered);
                    attempts = 0;
                }
                if !delegate_stop {
                    continue;
                }
            }
            if !delegate_stop && last_progress.elapsed() < STALL {
                continue;
            }
            if attempts == 0 {
                tracing::warn!(delegate_stop, "system audio stalled; restarting capture");
                let _ = status_tx.send(SystemStatus::Stalled);
            }
            if attempts >= MAX_ATTEMPTS {
                tracing::error!(attempts, "system audio restarts failed; giving up");
                let _ = status_tx.send(SystemStatus::Failed);
                return;
            }
            if attempts > 0 {
                let backoff = (BACKOFF_BASE * 2u32.pow(attempts - 1)).min(BACKOFF_CAP);
                tokio::time::sleep(backoff).await;
            }
            attempts += 1;
            stats.restarts.fetch_add(1, Ordering::Relaxed);
            match src.restart().await {
                Ok(()) => {
                    seen = stats.callbacks.load(Ordering::Relaxed);
                    last_progress = Instant::now();
                }
                // last_progress stays old, so the next tick retries after backoff.
                Err(e) => tracing::warn!(error = %e, attempts, "system audio restart failed"),
            }
        }
    }

    /// Outcome of mixing one SCK callback's buffers.
    #[derive(Debug, PartialEq, Eq)]
    pub(super) enum Mixed {
        /// Mono samples handed to the sink (0 for an empty buffer list).
        Samples(usize),
        /// A buffer was empty or not a whole number of f32 samples.
        BadFormat,
    }

    /// CR-04: SCK's parallel per-channel byte buffers to channel-mean mono f32,
    /// handed to `push` in slices of at most 1024 samples. The safe SCK surface
    /// exposes no ASBD, so a buffer that is empty or not a multiple of 4 bytes
    /// is the only detectable sign of a non-f32 format. Shorter channels cap
    /// the sample count.
    pub(super) fn mix_buffers(bufs: &[&[u8]], mut push: impl FnMut(&[f32])) -> Mixed {
        if bufs.is_empty() {
            return Mixed::Samples(0);
        }
        if bufs.iter().any(|b| b.is_empty() || b.len() % 4 != 0) {
            return Mixed::BadFormat;
        }
        let n = bufs.iter().map(|b| b.len() / 4).min().unwrap_or(0);
        let inv_ch = 1.0_f32 / bufs.len() as f32;
        let mut mono = [0.0_f32; 1024];
        for start in (0..n).step_by(mono.len()) {
            let len = (n - start).min(mono.len());
            for (i, out) in mono[..len].iter_mut().enumerate() {
                let off = (start + i) * 4;
                let sum: f32 = bufs
                    .iter()
                    .map(|b| f32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]]))
                    .sum();
                *out = sum * inv_ch;
            }
            push(&mono[..len]);
        }
        Mixed::Samples(n)
    }

    /// Wakes the watchdog when SCK reports the stream stopped.
    struct StopDelegate(Arc<Notify>);

    impl SCStreamDelegateTrait for StopDelegate {
        fn did_stop_with_error(&self, error: SCError) {
            tracing::warn!(error = %error, "SCStream stopped");
            self.0.notify_one();
        }
    }

    struct Slot {
        stream: Option<SCStream>,
        closed: bool,
    }

    /// Internal handle: owns the SCStream slot, the watchdog and the drainer.
    /// Drop calls stop_capture() explicitly (BL-04), waits 50 ms for in-flight
    /// callbacks to drain, then aborts the drainer.
    pub(super) struct Inner {
        slot: Arc<Mutex<Slot>>,
        supervisor: JoinHandle<()>,
        drainer: JoinHandle<()>,
    }

    impl Drop for Inner {
        fn drop(&mut self) {
            // BL-04: screencapturekit 8.x's SCStream::Drop is fire-and-forget,
            // so stop explicitly and let in-flight delegate calls drain before
            // the drainer (which owns the broadcast sender) goes away.
            self.supervisor.abort();
            {
                // A restart may be mid-build on a blocking thread; the lock
                // waits for it and `closed` keeps it from installing a stream.
                let mut slot = self.slot.lock();
                slot.closed = true;
                if let Some(stream) = slot.stream.take() {
                    if let Err(e) = stream.stop_capture() {
                        tracing::warn!(error = %e, "SCStream::stop_capture() failed during drop");
                    }
                }
            }
            std::thread::sleep(STOP_CAPTURE_DRAIN_WAIT);
            self.drainer.abort();
        }
    }

    impl std::fmt::Debug for Inner {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("Inner").finish_non_exhaustive()
        }
    }

    /// The first display is re-resolved on every build: a display that went
    /// away (lid closed, monitor unplugged) is the usual reason a stream dies.
    fn build_stream(
        producer: &Arc<Mutex<AudioRingProducer>>,
        stats: &Arc<SysStats>,
        stopped: &Arc<Notify>,
    ) -> Result<SCStream> {
        let content = SCShareableContent::get().map_err(|e| {
            AudioError::SystemCaptureFailed(format!("SCShareableContent::get(): {e}"))
        })?;
        let displays = content.displays();
        let display = displays.first().ok_or_else(|| {
            AudioError::SystemCaptureFailed("no displays available for SCK content filter".into())
        })?;

        let filter = SCContentFilter::create()
            .with_display(display)
            .with_excluding_windows(&[])
            .build();

        // Quirk 1: an audio-only stream still needs a valid video size.
        // AUDIO-03: exclude our own process audio or UI sounds loop back in.
        let config = SCStreamConfiguration::new()
            .with_width(2)
            .with_height(2)
            .with_captures_audio(true)
            .with_excludes_current_process_audio(true)
            .with_sample_rate(SCK_INPUT_RATE as i32)
            .with_channel_count(SCK_INPUT_CHANNELS as i32);

        let mut stream =
            SCStream::new_with_delegate(&filter, &config, StopDelegate(Arc::clone(stopped)));

        // The handler does only the real-time-safe minimum: mix to mono and
        // push to the ring. No broadcast send, no unbounded allocation.
        let producer = Arc::clone(producer);
        let stats = Arc::clone(stats);
        let handler = move |sample_buffer: CMSampleBuffer, of_type: SCStreamOutputType| {
            if of_type != SCStreamOutputType::Audio {
                return;
            }
            let Some(abl) = sample_buffer.audio_buffer_list() else {
                return;
            };
            stats.callbacks.fetch_add(1, Ordering::Relaxed);
            let bufs: Vec<&[u8]> = abl.iter().map(|b| b.data()).collect();
            let mut producer = producer.lock();
            if mix_buffers(&bufs, |s| producer.push_slice(s)) == Mixed::BadFormat {
                tracing::error!("SCK audio buffer is not packed native f32, dropping");
            }
        };
        stream
            .add_output_handler(handler, SCStreamOutputType::Audio)
            .ok_or_else(|| {
                AudioError::SystemCaptureFailed("SCStream::add_output_handler returned None".into())
            })?;

        stream.start_capture().map_err(|e| {
            // CR-02: classify TCC-flavoured errors back to PermissionDenied
            // so the recovery card renders even when the preflight check
            // returned a stale "granted".
            let msg = e.to_string();
            if msg.contains("declined") || msg.contains("TCC") || msg.contains("permission") {
                AudioError::PermissionDenied
            } else {
                AudioError::SystemCaptureFailed(format!("SCStream::start_capture: {msg}"))
            }
        })?;
        Ok(stream)
    }

    struct SckSource {
        slot: Arc<Mutex<Slot>>,
        producer: Arc<Mutex<AudioRingProducer>>,
        stats: Arc<SysStats>,
        stopped: Arc<Notify>,
    }

    impl Restart for SckSource {
        async fn restart(&mut self) -> Result<()> {
            let (slot, producer, stats, stopped) = (
                Arc::clone(&self.slot),
                Arc::clone(&self.producer),
                Arc::clone(&self.stats),
                Arc::clone(&self.stopped),
            );
            tokio::task::spawn_blocking(move || {
                let mut slot = slot.lock();
                if slot.closed {
                    return Ok(());
                }
                if let Some(old) = slot.stream.take() {
                    if let Err(e) = old.stop_capture() {
                        tracing::debug!(error = %e, "stopping the stalled SCStream failed");
                    }
                }
                slot.stream = Some(build_stream(&producer, &stats, &stopped)?);
                Ok(())
            })
            .await
            .map_err(|e| AudioError::SystemCaptureFailed(format!("restart task: {e}")))?
        }
    }

    /// Owns Downmix + FrameChunker and may safely block on broadcast::send.
    pub(super) fn spawn_drainer(
        mut consumer: AudioRingConsumer,
        mut downmix: Downmix,
        tx: broadcast::Sender<Frame>,
        stats: Arc<SysStats>,
    ) -> JoinHandle<()> {
        let mut chunker = FrameChunker::new(Channel::System, tx);
        tokio::spawn(async move {
            let mut scratch: Vec<f32> = Vec::with_capacity(DRAINER_SCRATCH_HINT);
            let mut interval = tokio::time::interval(DRAINER_TICK);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let mut last_logged_drops: u64 = 0;
            let mut last_summary = Instant::now();
            let mut last_callbacks = 0;
            loop {
                interval.tick().await;
                scratch.clear();
                let n = consumer.drain_to(&mut scratch);
                if n > 0 {
                    let i16_out = downmix.push(&scratch);
                    if !i16_out.is_empty() {
                        chunker.feed(&i16_out);
                    }
                }
                let drops = consumer.dropped_samples();
                if drops > last_logged_drops {
                    tracing::warn!(
                        dropped_samples = drops - last_logged_drops,
                        cumulative = drops,
                        channel = "system",
                        "audio drainer fell behind; SPSC ring overflowed"
                    );
                    last_logged_drops = drops;
                }
                if last_summary.elapsed() >= SUMMARY_EVERY {
                    let callbacks = stats.callbacks.load(Ordering::Relaxed);
                    tracing::info!(
                        sck_buffers = callbacks - last_callbacks,
                        ring_dropped_samples = drops,
                        restarts = stats.restarts.load(Ordering::Relaxed),
                        "system audio summary (last 30s)"
                    );
                    last_callbacks = callbacks;
                    last_summary = Instant::now();
                }
            }
        })
    }

    pub(super) fn start(
        tx: broadcast::Sender<Frame>,
        status_tx: broadcast::Sender<SystemStatus>,
    ) -> Result<Inner> {
        // D-25, CR-02: fast-fail on a FRESH permission check so mid-session
        // TCC revocation is not converted to a generic SystemCaptureFailed.
        if has_screen_recording_permission() == PermissionStatus::Denied {
            return Err(AudioError::PermissionDenied);
        }

        // The producer sits in a Mutex because the SCK handler must be
        // `Fn + Send + Sync` and rtrb::Producer is `!Sync`. SCK serializes
        // audio callbacks, so the lock is effectively uncontended.
        let (producer, consumer) = ring();
        let producer = Arc::new(Mutex::new(producer));
        let stats = Arc::new(SysStats::default());
        let stopped = Arc::new(Notify::new());

        let stream = build_stream(&producer, &stats, &stopped)?;
        let slot = Arc::new(Mutex::new(Slot {
            stream: Some(stream),
            closed: false,
        }));

        let drainer = spawn_drainer(
            consumer,
            Downmix::new(SCK_INPUT_RATE, 1)?,
            tx,
            Arc::clone(&stats),
        );
        let supervisor = tokio::spawn(supervise(
            SckSource {
                slot: Arc::clone(&slot),
                producer,
                stats: Arc::clone(&stats),
                stopped: Arc::clone(&stopped),
            },
            stats,
            stopped,
            status_tx,
        ));

        tracing::info!(
            sample_rate = SCK_INPUT_RATE,
            channels = SCK_INPUT_CHANNELS,
            "system audio capture running"
        );

        Ok(Inner {
            slot,
            supervisor,
            drainer,
        })
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::sync::atomic::AtomicBool;

        fn bytes(samples: &[f32]) -> Vec<u8> {
            samples.iter().flat_map(|s| s.to_le_bytes()).collect()
        }

        fn mix(bufs: &[&[u8]]) -> (Mixed, Vec<f32>) {
            let mut out = Vec::new();
            let r = mix_buffers(bufs, |s| out.extend_from_slice(s));
            (r, out)
        }

        #[test]
        fn mix_empty_buffer_list_pushes_nothing() {
            assert_eq!(mix(&[]), (Mixed::Samples(0), vec![]));
        }

        #[test]
        fn mix_averages_parallel_channels() {
            let (l, r) = (bytes(&[1.0, 0.5]), bytes(&[0.0, -0.5]));
            assert_eq!(mix(&[&l, &r]), (Mixed::Samples(2), vec![0.5, 0.0]));
        }

        #[test]
        fn mix_mixed_lengths_use_the_shortest_channel() {
            let (l, r) = (bytes(&[1.0, 1.0, 1.0]), bytes(&[1.0]));
            assert_eq!(mix(&[&l, &r]), (Mixed::Samples(1), vec![1.0]));
        }

        #[test]
        fn mix_rejects_odd_length_and_empty_buffers() {
            let good = bytes(&[1.0]);
            assert_eq!(mix(&[&[0u8; 6]]).0, Mixed::BadFormat);
            assert_eq!(mix(&[&good, &[]]).0, Mixed::BadFormat);
        }

        #[test]
        fn mix_rejects_i16_sized_buffers() {
            // 3 mono i16 samples is 6 bytes, which is not a whole f32 count.
            let i16s: Vec<u8> = [1i16, 2, 3].iter().flat_map(|s| s.to_le_bytes()).collect();
            let (r, out) = mix(&[&i16s]);
            assert_eq!(r, Mixed::BadFormat);
            assert!(out.is_empty());
        }

        #[test]
        fn mix_chunks_long_buffers() {
            let long = bytes(&vec![0.25; 2500]);
            let mut sizes = Vec::new();
            let r = mix_buffers(&[&long], |s| sizes.push(s.len()));
            assert_eq!(r, Mixed::Samples(2500));
            assert_eq!(sizes, vec![1024, 1024, 452]);
        }

        /// Pushes 10 ms of tone per millisecond-tick while `alive`, like SCK would.
        struct Fake {
            alive: Arc<AtomicBool>,
            restarts: Arc<Mutex<Vec<Instant>>>,
            /// What a restart does: bring audio back, or not.
            recovers: bool,
            /// Fail the restart call itself.
            errors: bool,
        }

        impl Restart for Fake {
            async fn restart(&mut self) -> Result<()> {
                self.restarts.lock().push(Instant::now());
                if self.errors {
                    return Err(AudioError::SystemCaptureFailed("boom".into()));
                }
                self.alive.store(self.recovers, Ordering::SeqCst);
                Ok(())
            }
        }

        struct Rig {
            alive: Arc<AtomicBool>,
            restarts: Arc<Mutex<Vec<Instant>>>,
            stopped: Arc<Notify>,
            frames: broadcast::Receiver<Frame>,
            status: broadcast::Receiver<SystemStatus>,
        }

        /// Real ring + drainer + watchdog around a fake SCK. `frames` is the
        /// one broadcast receiver that must keep working across restarts.
        fn rig(recovers: bool, errors: bool) -> Rig {
            let (frame_tx, frames) = broadcast::channel(512);
            let (status_tx, status) = broadcast::channel(8);
            let (mut producer, consumer) = ring();
            let stats = Arc::new(SysStats::default());
            let stopped = Arc::new(Notify::new());
            let alive = Arc::new(AtomicBool::new(true));
            let restarts = Arc::new(Mutex::new(Vec::new()));
            spawn_drainer(
                consumer,
                Downmix::new(SCK_INPUT_RATE, 1).unwrap(),
                frame_tx,
                Arc::clone(&stats),
            );
            tokio::spawn({
                let (alive, stats) = (Arc::clone(&alive), Arc::clone(&stats));
                async move {
                    let mut t = tokio::time::interval(Duration::from_millis(10));
                    loop {
                        t.tick().await;
                        if alive.load(Ordering::SeqCst) {
                            stats.callbacks.fetch_add(1, Ordering::Relaxed);
                            producer.push_slice(&[0.5; 480]);
                        }
                    }
                }
            });
            tokio::spawn(supervise(
                Fake {
                    alive: Arc::clone(&alive),
                    restarts: Arc::clone(&restarts),
                    recovers,
                    errors,
                },
                stats,
                Arc::clone(&stopped),
                status_tx,
            ));
            Rig {
                alive,
                restarts,
                stopped,
                frames,
                status,
            }
        }

        fn drain(rx: &mut broadcast::Receiver<Frame>) -> usize {
            let mut n = 0;
            while rx.try_recv().is_ok() {
                n += 1;
            }
            n
        }

        #[tokio::test(start_paused = true)]
        async fn stall_restarts_and_frames_resume_on_the_same_sender() {
            let mut r = rig(true, false);
            tokio::time::sleep(Duration::from_secs(2)).await;
            assert!(drain(&mut r.frames) > 50, "frames flow before the stall");
            assert!(r.status.try_recv().is_err());

            r.alive.store(false, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(500)).await;
            drain(&mut r.frames);
            tokio::time::sleep(Duration::from_secs(3)).await;
            assert_eq!(drain(&mut r.frames), 0, "silent while stalled");
            assert!(r.restarts.lock().is_empty(), "no restart before STALL");

            tokio::time::sleep(Duration::from_secs(4)).await;
            assert_eq!(r.restarts.lock().len(), 1);
            assert_eq!(r.status.try_recv(), Ok(SystemStatus::Stalled));
            assert_eq!(r.status.try_recv(), Ok(SystemStatus::Recovered));
            assert!(drain(&mut r.frames) > 50, "frames resume after the restart");
        }

        #[tokio::test(start_paused = true)]
        async fn delegate_stop_restarts_without_waiting_for_the_stall_window() {
            let mut r = rig(true, false);
            tokio::time::sleep(Duration::from_secs(1)).await;
            r.stopped.notify_one();
            tokio::time::sleep(Duration::from_secs(1)).await;
            assert_eq!(r.restarts.lock().len(), 1);
            assert_eq!(r.status.try_recv(), Ok(SystemStatus::Stalled));
        }

        #[tokio::test(start_paused = true)]
        async fn failed_restarts_back_off_then_report_permanent_failure() {
            let mut r = rig(false, true);
            r.alive.store(false, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_secs(300)).await;

            let times = r.restarts.lock().clone();
            assert_eq!(times.len(), MAX_ATTEMPTS as usize, "capped, no spinning");
            let gaps: Vec<u64> = times.windows(2).map(|w| (w[1] - w[0]).as_secs()).collect();
            // The 1 s watchdog tick adds up to a second on top of each backoff.
            for (i, gap) in gaps.iter().enumerate() {
                let backoff = 1u64 << i;
                assert!(
                    (backoff..=backoff + 1).contains(gap),
                    "gap {i} was {gap}s, expected about {backoff}s"
                );
            }
            assert_eq!(r.status.try_recv(), Ok(SystemStatus::Stalled));
            assert_eq!(r.status.try_recv(), Ok(SystemStatus::Failed));
            assert!(r.status.try_recv().is_err());
        }

        #[tokio::test(start_paused = true)]
        async fn restart_that_brings_no_audio_counts_toward_the_cap() {
            let mut r = rig(false, false);
            r.alive.store(false, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_secs(600)).await;
            assert_eq!(r.restarts.lock().len(), MAX_ATTEMPTS as usize);
            assert_eq!(r.status.try_recv(), Ok(SystemStatus::Stalled));
            assert_eq!(r.status.try_recv(), Ok(SystemStatus::Failed));
        }
    }
}

#[cfg(all(test, not(target_os = "macos")))]
mod tests {
    use super::*;

    #[test]
    fn it_returns_unsupported_platform_off_macos() {
        let (tx, _rx) = broadcast::channel::<Frame>(8);
        let err = spawn_system_capture(tx).unwrap_err();
        assert!(matches!(err, AudioError::UnsupportedPlatform));
    }
}
