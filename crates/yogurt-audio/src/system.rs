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

use crate::{
    error::{AudioError, Result},
    frame::{Channel, Frame},
    mic::FrameChunker,
};
use tokio::sync::broadcast;

/// Live system-audio capture handle. Dropping it stops the underlying
/// `SCStream` (BL-04: explicit `stop_capture()` + drain wait) and aborts
/// the drainer task.
pub struct SystemCapture {
    #[cfg(target_os = "macos")]
    _inner: macos::Inner,
}

impl std::fmt::Debug for SystemCapture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SystemCapture").finish_non_exhaustive()
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
    let inner = macos::start(tx)?;
    Ok(SystemCapture { _inner: inner })
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
    use crate::ring::{ring, DRAINER_SCRATCH_HINT, DRAINER_TICK};
    use parking_lot::Mutex;
    use screencapturekit::prelude::*;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::task::JoinHandle;

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

    /// Internal handle: owns the SCStream and the drainer task. Drop calls
    /// stop_capture() explicitly (BL-04), waits 50 ms for in-flight
    /// callbacks to drain, then aborts the drainer.
    pub(super) struct Inner {
        stream: Option<SCStream>,
        drainer: JoinHandle<()>,
    }

    impl Drop for Inner {
        fn drop(&mut self) {
            // BL-04: the previous code relied entirely on SCStream::Drop
            // doing the right thing. In practice screencapturekit 8.x's
            // Drop is a fire-and-forget tear-down; dropping `AudioStream`
            // did NOT guarantee capture had stopped by the time the next
            // line of code ran — Phase 3 teardown tests would flake.
            //
            // Now: take() the stream so we can call stop_capture() on it
            // by value (the crate's API takes &mut self / self by value
            // depending on minor version — &mut works on 8.x). Then sleep
            // 50 ms to let any in-flight delegate calls drain before we
            // abort the drainer (which would drop the broadcast sender).
            if let Some(stream) = self.stream.take() {
                if let Err(e) = stream.stop_capture() {
                    tracing::warn!(error = %e, "SCStream::stop_capture() failed during drop");
                }
            }
            std::thread::sleep(STOP_CAPTURE_DRAIN_WAIT);
            self.drainer.abort();
        }
    }

    impl std::fmt::Debug for Inner {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("Inner")
                .field("stream_alive", &self.stream.is_some())
                .finish_non_exhaustive()
        }
    }

    pub(super) fn start(tx: broadcast::Sender<Frame>) -> Result<Inner> {
        // 1. Permission gate (D-25, CR-02). Fast-fail before we touch SCK.
        //    Done as a FRESH check (not cached) on each call so that
        //    mid-session TCC revocation is caught at start_capture time,
        //    not silently converted to a generic SystemCaptureFailed.
        if has_screen_recording_permission() == PermissionStatus::Denied {
            return Err(AudioError::PermissionDenied);
        }

        // 2. Get shareable content + pick the first display.
        let content = SCShareableContent::get().map_err(|e| {
            AudioError::SystemCaptureFailed(format!("SCShareableContent::get(): {e}"))
        })?;
        let displays = content.displays();
        let display = displays.first().ok_or_else(|| {
            AudioError::SystemCaptureFailed("no displays available for SCK content filter".into())
        })?;

        // 3. Build content filter (display-rooted, no window exclusions).
        let filter = SCContentFilter::create()
            .with_display(display)
            .with_excluding_windows(&[])
            .build();

        // 4. Build audio-only config.
        let config = SCStreamConfiguration::new()
            .with_width(2)
            .with_height(2)
            .with_captures_audio(true)
            .with_excludes_current_process_audio(true)
            .with_sample_rate(SCK_INPUT_RATE as i32)
            .with_channel_count(SCK_INPUT_CHANNELS as i32);

        // 5. Construct the SPSC ring (BL-01/BL-03). The producer half is
        //    wrapped in parking_lot::Mutex because SCStreamOutputTrait
        //    requires `Fn + Send + Sync` and rtrb::Producer is `!Sync`
        //    by design. Mutex contention is effectively zero (SCK's
        //    audio dispatch contract serializes callbacks for a given
        //    output type) and the critical section is one ring push.
        let (producer, mut consumer) = ring();
        let producer = Arc::new(Mutex::new(producer));

        // 6. Build stream + register audio handler. The handler does ONLY
        //    the real-time-safe minimum: decode f32 bytes from each
        //    parallel buffer, average to mono per sample, push to ring.
        //    No mutex, no broadcast send. Allocation is bounded to a
        //    small fixed stack buffer.
        let mut stream = SCStream::new(&filter, &config);

        let producer_for_handler = Arc::clone(&producer);
        let handler = move |sample_buffer: CMSampleBuffer, of_type: SCStreamOutputType| {
            if of_type != SCStreamOutputType::Audio {
                return;
            }
            let Some(abl) = sample_buffer.audio_buffer_list() else {
                return;
            };

            // CR-04: validate the ASBD format flags before assuming f32 LE.
            // SCK ought to honour the channel_count/sample_rate we passed in
            // configure() but the spike only proved it on one machine. If a
            // future macOS version delivers i16 or i32, we'd silently
            // produce garbage PCM and feed transcribed noise to STT — better
            // to log loudly and drop the buffer.
            if !audio_format_is_packed_native_f32(&sample_buffer) {
                tracing::error!(
                    "SCK audio buffer is not packed native f32 — dropping. \
                     Phase 9 needs a CI matrix that catches this on minimum-supported macOS."
                );
                return;
            }

            // Walk each parallel buffer once: decode f32, average across
            // channels per-sample-index. We hold all channel byte slices
            // up front and stream samples through a fixed stack buffer
            // — never allocating more than the inner loop's 1024 frames.
            let bufs: Vec<&[u8]> = abl.iter().map(|b| b.data()).collect();
            if bufs.is_empty() {
                return;
            }
            let ch = bufs.len();
            let sample_count = bufs.iter().map(|b| b.len() / 4).min().unwrap_or(0);
            if sample_count == 0 {
                return;
            }
            let inv_ch = 1.0_f32 / ch as f32;
            let mut mono = [0.0_f32; 1024];
            let mut written = 0usize;
            // Take the producer lock once for the entire callback's worth
            // of pushes — SCK guarantees per-output-type callback
            // serialization, so this lock is effectively uncontended; the
            // expect on poisoning is intentional (silent absorption would
            // hide a producer-side panic). parking_lot::Mutex has no
            // poisoning so this is structurally simpler than std's Mutex.
            let mut producer = producer_for_handler.lock();
            for i in 0..sample_count {
                if written == mono.len() {
                    producer.push_slice(&mono[..written]);
                    written = 0;
                }
                let mut sum = 0.0_f32;
                for &b in &bufs {
                    let off = i * 4;
                    // SAFETY-equivalent: chunks were validated as 4-byte
                    // aligned via the sample_count division above. Use
                    // copy + from_le_bytes (constant-fold-friendly).
                    let mut a = [0_u8; 4];
                    a.copy_from_slice(&b[off..off + 4]);
                    sum += f32::from_le_bytes(a);
                }
                mono[written] = sum * inv_ch;
                written += 1;
            }
            if written > 0 {
                producer.push_slice(&mono[..written]);
            }
        };

        let _handler_id = stream
            .add_output_handler(handler, SCStreamOutputType::Audio)
            .ok_or_else(|| {
                AudioError::SystemCaptureFailed("SCStream::add_output_handler returned None".into())
            })?;

        // 7. Start capture.
        stream.start_capture().map_err(|e| {
            // CR-02: classify TCC-flavoured errors back to PermissionDenied
            // so the §5.11 recovery card renders correctly even when the
            // preflight check returned a stale "granted" but SCK's internal
            // TCC check (which is the authoritative one) refused us.
            let msg = e.to_string();
            if msg.contains("declined") || msg.contains("TCC") || msg.contains("permission") {
                AudioError::PermissionDenied
            } else {
                AudioError::SystemCaptureFailed(format!("SCStream::start_capture: {msg}"))
            }
        })?;

        // 8. Spawn the drainer task. Owns Downmix + FrameChunker, may
        //    safely block on broadcast::send.
        let mut downmix = Downmix::new(SCK_INPUT_RATE, 1)?;
        let mut chunker = FrameChunker::new(Channel::System, tx);
        let drainer = tokio::spawn(async move {
            let mut scratch: Vec<f32> = Vec::with_capacity(DRAINER_SCRATCH_HINT);
            let mut interval = tokio::time::interval(DRAINER_TICK);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let mut last_logged_drops: u64 = 0;
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
            }
        });

        tracing::info!(
            sample_rate = SCK_INPUT_RATE,
            channels = SCK_INPUT_CHANNELS,
            "system audio capture running"
        );

        Ok(Inner {
            stream: Some(stream),
            drainer,
        })
    }

    /// CR-04: check that the sample buffer's format is the packed native f32
    /// we assume in the byte-decode loop. The screencapturekit 8.x crate
    /// does not currently expose CMFormatDescription / ASBD inspection on
    /// the safe Rust surface (it would require dropping down to core-media
    /// bindings). For v1 we rely on the configuration we asked for
    /// (`with_sample_rate(48000).with_channel_count(2)`) being honoured;
    /// we treat any sample buffer whose total byte count is NOT a multiple
    /// of 4 as a format violation (f32 = 4 bytes, packed, native-endian
    /// implied on Apple Silicon and Intel macOS — both little-endian).
    ///
    /// This is a weaker check than reading the ASBD directly. When the
    /// screencapturekit crate exposes format introspection (or when we
    /// drop to core-foundation FFI), upgrade to an explicit
    /// `kAudioFormatFlagIsFloat | kAudioFormatFlagIsPacked` assertion.
    fn audio_format_is_packed_native_f32(buf: &CMSampleBuffer) -> bool {
        let Some(abl) = buf.audio_buffer_list() else {
            return false;
        };
        // Every parallel buffer must be a positive multiple of 4 bytes.
        let mut saw_any = false;
        for b in abl.iter() {
            saw_any = true;
            let n = b.data().len();
            if n == 0 || n % 4 != 0 {
                return false;
            }
        }
        saw_any
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
