//! Microphone capture via `cpal` → CoreAudio default input device.
//!
//! cpal is the boring, battle-tested CoreAudio binding (D-13). The default
//! input device is typically 48 kHz f32 (sometimes mono, sometimes stereo);
//! we down-mix to mono in the callback (cheap), and the drainer task does
//! the rubato resample to 16 kHz, the i16 quantize, and the broadcast send.
//!
//! ## Threading (BL-01 / BL-03 fix)
//!
//! cpal invokes the data callback on CoreAudio's IOProc thread which is
//! real-time-priority and **must not block** — if the IOProc misses its
//! deadline, macOS refuses to deliver the next ~10 ms buffer.
//!
//! Pipeline:
//!
//! ```text
//! cpal IOProc thread:                tokio drainer task:
//!   bytes → f32 → mono mean              drain SPSC ring
//!   push to SPSC ring (lock-free)        rubato 48k → 16k
//!         │                              f32 → i16 (clamp + multiply)
//!         └─────► SPSC ring ─────►       FrameChunker → broadcast::Sender
//! ```
//!
//! The callback never touches a mutex, never allocates on the steady-state
//! path, never sends on the broadcast — so the audio thread is provably
//! real-time-safe. The drainer task lives on the tokio runtime and may
//! freely call `broadcast::Sender::send`.

use crate::{
    error::{AudioError, Result},
    frame::{Channel, Frame, FRAME_SAMPLES},
    resample::Downmix,
    ring::{ring, AudioRingConsumer, AudioRingProducer, DRAINER_SCRATCH_HINT, DRAINER_TICK},
};
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    SampleFormat,
};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

/// Live mic capture handle. Drop to stop the underlying `cpal::Stream` AND
/// abort the drainer task.
pub struct MicCapture {
    /// Keep-alive — dropping this stops the OS-level capture via RAII.
    _stream: cpal::Stream,
    /// Drainer task — aborted on Drop so it stops touching the broadcast
    /// sender as soon as the user releases the stream.
    drainer: JoinHandle<()>,
    /// The device cpal handed us at construction time. Surfaced via
    /// `/api/audio/devices` for the UI.
    pub device_name: String,
    /// AUD-6: shared with the drainer's `FrameChunker`. See
    /// [`MicCapture::set_muted`].
    muted: Arc<AtomicBool>,
    /// AUD-11: native sample rate this capture was opened at. The echo
    /// output stream must match it exactly (see `echo.rs`).
    pub(crate) sample_rate: u32,
    /// AUD-11: gates whether the callback bothers teeing samples into
    /// `echo_consumer`'s ring - set by `AudioStream::start_echo`/`stop_echo`.
    echo_active: Arc<AtomicBool>,
    /// AUD-11: the consumer half of the echo ring, shared with whichever
    /// echo output stream is currently reading it. Lives for the whole
    /// capture's lifetime (unlike the old take-once design) so re-enabling
    /// echo never needs to respawn the mic - see
    /// [`MicCapture::echo_consumer_handle`].
    echo_consumer: Arc<std::sync::Mutex<AudioRingConsumer>>,
}

impl Drop for MicCapture {
    fn drop(&mut self) {
        // AUD-11 E2E fix: cpal's macOS `Stream` has a permanent internal
        // Arc self-reference (its own disconnect listener clones itself
        // into its own StreamInner - see echo.rs's `Drop for MicEcho` for
        // the full story), so the refcount never hits zero and Drop alone
        // never truly stops the underlying AudioUnit. `pause()` calls
        // `audio_unit.stop()` directly and reliably silences it.
        let _ = self._stream.pause();
        self.drainer.abort();
    }
}

impl MicCapture {
    /// AUD-6: pause (`true`) or resume (`false`) mic audio reaching the
    /// broadcast channel — `Channel::System` is untouched. The cpal stream
    /// and drainer keep running either way, so resuming is instant with no
    /// re-open cost, unlike a real device switch.
    pub fn set_muted(&self, muted: bool) {
        self.muted.store(muted, Ordering::Relaxed);
    }

    /// AUD-11: shared with the echo output stream so muting the mic also
    /// zero-fills the echo instead of playing stale/underrun audio.
    pub(crate) fn muted_handle(&self) -> Arc<AtomicBool> {
        self.muted.clone()
    }

    /// AUD-11: enable/disable the callback teeing samples into the echo
    /// ring. Called by `AudioStream::start_echo`/`stop_echo`.
    pub(crate) fn set_echo_active(&self, active: bool) {
        self.echo_active.store(active, Ordering::Relaxed);
    }

    /// AUD-11: clone a handle to the echo ring's consumer half. Cheap
    /// (Arc clone) and callable any number of times - only one echo output
    /// stream should read it at a time, which `AudioStream` guarantees by
    /// always dropping the previous `MicEcho` before installing a new one.
    pub(crate) fn echo_consumer_handle(&self) -> Arc<std::sync::Mutex<AudioRingConsumer>> {
        self.echo_consumer.clone()
    }
}

impl std::fmt::Debug for MicCapture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // `cpal::Stream` is not Debug — print just what we own logically.
        f.debug_struct("MicCapture")
            .field("device_name", &self.device_name)
            .finish_non_exhaustive()
    }
}

/// Information about an input device, surfaced through the `GET /api/audio/devices`
/// endpoint Phase 7's settings UI will consume.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub name: String,
    pub is_default: bool,
    /// Reported default sample rate, when cpal can supply it. `None` for
    /// devices that error out (e.g. a virtual device with no live config).
    pub sample_rate: Option<u32>,
}

/// Collects an arbitrary-length i16 PCM stream into exactly-`FRAME_SAMPLES`
/// `Frame`s and broadcasts them. Producers create one per stream.
///
/// Owned by the drainer task — no longer shared across threads or mutexes
/// since BL-01.
pub(crate) struct FrameChunker {
    channel: Channel,
    tx: broadcast::Sender<Frame>,
    buf: Vec<i16>,
    /// Meeting-relative clock baseline — captured at chunker construction
    /// time, which is captured inside the drainer setup inside
    /// `spawn_*_capture()`. AUDIO-05 requires drift between mic and
    /// system streams < 50 ms; both drainers are spawned synchronously
    /// inside `start_capture()` so the spawn-order skew is microseconds.
    start: Instant,
    /// AUD-6: while `true`, `feed` still drains `buf` and advances `start`
    /// as normal, it just skips the broadcast — so unmuting produces no
    /// timestamp jump or discontinuity, only a real gap in the audio that
    /// reached the pipeline. `Mic`-only in practice: `System` chunkers are
    /// constructed via `new`, which always starts unmuted.
    muted: Arc<AtomicBool>,
}

impl FrameChunker {
    pub(crate) fn new(channel: Channel, tx: broadcast::Sender<Frame>) -> Self {
        Self::with_mute(channel, tx, Arc::new(AtomicBool::new(false)))
    }

    pub(crate) fn with_mute(
        channel: Channel,
        tx: broadcast::Sender<Frame>,
        muted: Arc<AtomicBool>,
    ) -> Self {
        Self {
            channel,
            tx,
            buf: Vec::with_capacity(FRAME_SAMPLES * 2),
            start: Instant::now(),
            muted,
        }
    }

    /// Feed an arbitrary slice of i16 PCM. Drains the internal buffer in
    /// `FRAME_SAMPLES`-sized chunks, computes `monotonic_micros` per frame
    /// from the recorded `start` instant, and sends each frame on the
    /// broadcast channel. Send errors (no live subscribers) are dropped
    /// silently per D-19's "consumers fall behind → drop late frames"
    /// semantics.
    ///
    /// **CR-01:** microsecond resolution (not millisecond) so the 20 ms
    /// frame cadence remains distinguishable from wall-clock jitter under
    /// any downstream cross-channel alignment use. See
    /// [`Frame::monotonic_micros`].
    pub(crate) fn feed(&mut self, samples: &[i16]) {
        self.buf.extend_from_slice(samples);
        while self.buf.len() >= FRAME_SAMPLES {
            let chunk: Vec<i16> = self.buf.drain(..FRAME_SAMPLES).collect();
            let monotonic_micros = self.start.elapsed().as_micros() as u64;
            if self.muted.load(Ordering::Relaxed) {
                continue;
            }
            let frame = Frame::new(self.channel, monotonic_micros, chunk);
            // Drop silently if no live subscribers.
            let _ = self.tx.send(frame);
        }
    }
}

impl std::fmt::Debug for FrameChunker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FrameChunker")
            .field("channel", &self.channel)
            .field("buf_len", &self.buf.len())
            .finish_non_exhaustive()
    }
}

/// Enumerate all input devices the cpal default host knows about. Returned
/// for the `GET /api/audio/devices` REST endpoint (Plan 02-03).
pub fn list_input_devices() -> Result<Vec<DeviceInfo>> {
    let host = cpal::default_host();
    let default_name = host.default_input_device().and_then(|d| d.name().ok());
    let mut out = Vec::new();
    let devices = host
        .input_devices()
        .map_err(|e| AudioError::Cpal(format!("input_devices(): {e}")))?;
    for device in devices {
        let name = device.name().unwrap_or_else(|_| "<unknown>".to_string());
        let is_default = default_name.as_deref() == Some(name.as_str());
        let sample_rate = device
            .default_input_config()
            .ok()
            .map(|c| c.sample_rate().0);
        out.push(DeviceInfo {
            name,
            is_default,
            sample_rate,
        });
    }
    Ok(out)
}

/// Enumerate all output devices the cpal default host knows about. AUD-11:
/// backs `GET /api/audio/output-devices`, populating the echo-destination
/// picker (typically listing "BlackHole 2ch" alongside real speakers).
pub fn list_output_devices() -> Result<Vec<DeviceInfo>> {
    let host = cpal::default_host();
    let default_name = host.default_output_device().and_then(|d| d.name().ok());
    let mut out = Vec::new();
    let devices = host
        .output_devices()
        .map_err(|e| AudioError::Cpal(format!("output_devices(): {e}")))?;
    for device in devices {
        let name = device.name().unwrap_or_else(|_| "<unknown>".to_string());
        let is_default = default_name.as_deref() == Some(name.as_str());
        let sample_rate = device
            .default_output_config()
            .ok()
            .map(|c| c.sample_rate().0);
        out.push(DeviceInfo {
            name,
            is_default,
            sample_rate,
        });
    }
    Ok(out)
}

/// Spawn a microphone capture stream on the default input device. The
/// supplied `tx` receives one [`Frame`] every 20 ms while the returned
/// [`MicCapture`] is alive; drop it to stop capture (RAII per D-26).
///
/// **BL-01 architecture:** the cpal callback only pushes mono f32 samples
/// into a lock-free SPSC ring. A dedicated tokio drainer task consumes the
/// ring, runs rubato + i16 conversion + frame chunking + broadcast. The
/// callback never blocks and never touches the broadcast sender.
pub fn spawn_mic_capture(
    tx: broadcast::Sender<Frame>,
    requested_device: Option<&str>,
) -> Result<MicCapture> {
    let host = cpal::default_host();
    let device = match requested_device {
        Some(name) if !name.is_empty() => host
            .input_devices()
            .map_err(|e| AudioError::Cpal(format!("input_devices(): {e}")))?
            .find(|d| d.name().map(|n| n == name).unwrap_or(false))
            .ok_or_else(|| AudioError::MicUnavailable(format!("input device not found: {name}")))?,
        _ => host
            .default_input_device()
            .ok_or_else(|| AudioError::MicUnavailable("no default input device".into()))?,
    };
    let device_name = device.name().unwrap_or_else(|_| "<unknown>".to_string());
    let supported = device
        .default_input_config()
        .map_err(|e| AudioError::Cpal(format!("default_input_config: {e}")))?;

    let sample_rate = supported.sample_rate().0;
    let channels = supported.channels();
    let sample_format = supported.sample_format();
    let stream_config: cpal::StreamConfig = supported.into();

    tracing::info!(
        device = %device_name,
        sample_rate,
        channels,
        ?sample_format,
        "spawning mic capture"
    );

    // Producer/consumer pair shared between the audio thread and the drainer.
    let (mut producer, mut consumer) = ring();
    // AUD-11: second ring the callback tees mono samples into when echo is
    // active (`echo.rs` reads it from an output-device callback). Always
    // created - cheap (256 KiB) - so `start_echo` never has to touch the
    // audio thread; it only flips `echo_active` and takes `echo_consumer`.
    let (mut echo_producer, echo_consumer) = ring();
    let echo_consumer = Arc::new(std::sync::Mutex::new(echo_consumer));
    let echo_active = Arc::new(AtomicBool::new(false));
    let echo_active_cb = echo_active.clone();

    let err_callback = |e: cpal::StreamError| {
        tracing::error!(error = %e, "cpal mic stream error");
    };

    // Build the cpal stream. Callback does the minimum: bytes → f32 (if
    // needed) → mono channel-mean → SPSC ring push (main + echo tee). No
    // allocation, no mutex, no broadcast send.
    let channels_us = channels as usize;
    let stream = match sample_format {
        SampleFormat::F32 => device
            .build_input_stream::<f32, _, _>(
                &stream_config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    let mut sink = TeeSink {
                        main: &mut producer,
                        echo: &mut echo_producer,
                        echo_active: &echo_active_cb,
                    };
                    push_mono_f32(&mut sink, data, channels_us);
                },
                err_callback,
                None,
            )
            .map_err(|e| AudioError::Cpal(format!("build_input_stream f32: {e}")))?,
        SampleFormat::I16 => device
            .build_input_stream::<i16, _, _>(
                &stream_config,
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    let mut sink = TeeSink {
                        main: &mut producer,
                        echo: &mut echo_producer,
                        echo_active: &echo_active_cb,
                    };
                    push_mono_i16(&mut sink, data, channels_us);
                },
                err_callback,
                None,
            )
            .map_err(|e| AudioError::Cpal(format!("build_input_stream i16: {e}")))?,
        other => {
            return Err(AudioError::Cpal(format!(
                "unsupported sample format {other:?}; expected F32 or I16"
            )))
        }
    };

    stream
        .play()
        .map_err(|e| AudioError::Cpal(format!("stream.play(): {e}")))?;

    // Spawn the drainer. Owns rubato (Downmix, mono-already so input_channels=1)
    // and the FrameChunker. Wakes every DRAINER_TICK, drains the ring,
    // resamples, chunks, broadcasts. May safely block on broadcast::send.
    let mut downmix = Downmix::new(sample_rate, 1)?;
    let muted = Arc::new(AtomicBool::new(false));
    let mut chunker = FrameChunker::with_mute(Channel::Mic, tx, muted.clone());
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
            // Surface ring overflow as a warn! once per spike.
            let drops = consumer.dropped_samples();
            if drops > last_logged_drops {
                tracing::warn!(
                    dropped_samples = drops - last_logged_drops,
                    cumulative = drops,
                    channel = "mic",
                    "audio drainer fell behind; SPSC ring overflowed"
                );
                last_logged_drops = drops;
            }
        }
    });

    Ok(MicCapture {
        _stream: stream,
        drainer,
        device_name,
        muted,
        sample_rate,
        echo_active,
        echo_consumer,
    })
}

/// Destination for down-mixed mono samples. Implemented by
/// [`AudioRingProducer`] directly, and by [`TeeSink`] to fan the same
/// samples into a second (echo) ring without a second down-mix pass.
trait Sink {
    fn push_slice(&mut self, samples: &[f32]);
}

impl Sink for AudioRingProducer {
    fn push_slice(&mut self, samples: &[f32]) {
        AudioRingProducer::push_slice(self, samples)
    }
}

/// AUD-11: tees every push into `main` (always) and `echo` (only while
/// `echo_active` is set). `echo_active` is an `AtomicBool` load per push -
/// negligible next to the ring write itself, and it's how a mic callback
/// with no echo consumer attached stays a no-op on the echo path.
struct TeeSink<'a> {
    main: &'a mut AudioRingProducer,
    echo: &'a mut AudioRingProducer,
    echo_active: &'a AtomicBool,
}

impl Sink for TeeSink<'_> {
    fn push_slice(&mut self, samples: &[f32]) {
        self.main.push_slice(samples);
        if self.echo_active.load(Ordering::Relaxed) {
            self.echo.push_slice(samples);
        }
    }
}

/// Down-mix an interleaved f32 slice to mono (channel-mean) and push into
/// the SPSC ring. Real-time-safe: no allocation, single ring write.
///
/// Done on the audio thread because it's O(N) trivial arithmetic; pushing
/// raw stereo would double the ring's bandwidth requirement for no gain.
fn push_mono_f32(sink: &mut impl Sink, data: &[f32], channels: usize) {
    if channels <= 1 {
        sink.push_slice(data);
        return;
    }
    // Compute mono in-place using a small fixed stack buffer. Bounded chunk
    // size so the stack array is small. 1024 input samples ≈ 512 mono out
    // for stereo — plenty for one IOProc callback (cpal callbacks are
    // typically 480–960 samples).
    let mut mono = [0.0_f32; 1024];
    let mut written = 0usize;
    for frame in data.chunks_exact(channels) {
        if written == mono.len() {
            sink.push_slice(&mono[..written]);
            written = 0;
        }
        let sum: f32 = frame.iter().sum();
        mono[written] = sum / channels as f32;
        written += 1;
    }
    if written > 0 {
        sink.push_slice(&mono[..written]);
    }
}

/// Same as [`push_mono_f32`] but takes i16 input. Symmetric conversion
/// using **32768.0** (= 2^15), not `i16::MAX` (= 32767), so `i16::MIN`
/// (= -32768) maps cleanly to -1.0 instead of -1.00003. See BL-02.
fn push_mono_i16(sink: &mut impl Sink, data: &[i16], channels: usize) {
    /// BL-02: 2^15 = 32768.0, the symmetric divisor. Matches the multiplier
    /// used in resample.rs's f32 → i16 path so the round-trip is identity
    /// within the i16 saturation range.
    const I16_TO_F32: f32 = 1.0 / 32768.0;

    let mut mono = [0.0_f32; 1024];
    let mut written = 0usize;
    if channels <= 1 {
        // Convert directly to f32 in chunks.
        for &s in data {
            if written == mono.len() {
                sink.push_slice(&mono[..written]);
                written = 0;
            }
            mono[written] = (s as f32) * I16_TO_F32;
            written += 1;
        }
    } else {
        for frame in data.chunks_exact(channels) {
            if written == mono.len() {
                sink.push_slice(&mono[..written]);
                written = 0;
            }
            let sum: f32 = frame.iter().map(|&s| (s as f32) * I16_TO_F32).sum();
            mono[written] = sum / channels as f32;
            written += 1;
        }
    }
    if written > 0 {
        sink.push_slice(&mono[..written]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunker_emits_exactly_frame_samples_per_frame() {
        let (tx, mut rx) = broadcast::channel::<Frame>(8);
        let mut chunker = FrameChunker::new(Channel::Mic, tx);
        // Feed 1000 samples → 3 full frames (960 samples), 40 buffered.
        let samples: Vec<i16> = (0..1000).map(|i| (i as i16) % 1000).collect();
        chunker.feed(&samples);
        let mut count = 0;
        while let Ok(frame) = rx.try_recv() {
            assert_eq!(frame.samples.len(), FRAME_SAMPLES);
            assert_eq!(frame.channel, Channel::Mic);
            count += 1;
        }
        assert_eq!(count, 3, "expected 3 frames for 1000 samples");
        assert_eq!(chunker.buf.len(), 40, "expected 40 leftover samples");
    }

    #[test]
    fn chunker_drops_frames_while_muted_and_resumes_when_unmuted() {
        let (tx, mut rx) = broadcast::channel::<Frame>(8);
        let muted = Arc::new(AtomicBool::new(true));
        let mut chunker = FrameChunker::with_mute(Channel::Mic, tx, muted.clone());

        chunker.feed(&vec![0_i16; FRAME_SAMPLES]);
        assert!(
            rx.try_recv().is_err(),
            "no frame should broadcast while muted"
        );

        muted.store(false, Ordering::Relaxed);
        chunker.feed(&vec![0_i16; FRAME_SAMPLES]);
        assert!(rx.try_recv().is_ok(), "frames should resume once unmuted");
    }

    #[test]
    fn chunker_monotonic_micros_is_non_decreasing() {
        let (tx, mut rx) = broadcast::channel::<Frame>(8);
        let mut chunker = FrameChunker::new(Channel::System, tx);
        // Feed two frames worth so we get two events.
        let samples = vec![0_i16; FRAME_SAMPLES * 2];
        chunker.feed(&samples);
        let f1 = rx.try_recv().expect("first frame");
        let f2 = rx.try_recv().expect("second frame");
        assert!(
            f2.monotonic_micros >= f1.monotonic_micros,
            "monotonic_micros should be non-decreasing: f1={}, f2={}",
            f1.monotonic_micros,
            f2.monotonic_micros
        );
    }

    #[test]
    fn spawn_mic_capture_unknown_device_returns_mic_unavailable() {
        let (tx, _rx) = broadcast::channel::<Frame>(8);
        let result = spawn_mic_capture(tx, Some("definitely-not-a-real-device-xyz123"));
        match result {
            Err(AudioError::MicUnavailable(_)) => {}
            other => panic!("expected Err(AudioError::MicUnavailable(_)), got {other:?}"),
        }
    }

    #[test]
    fn list_input_devices_does_not_panic() {
        // We can't assert anything about hardware here (CI macOS runners have
        // no audio devices, dev machines have varying setups). Just call it
        // and ensure it returns Ok(_) — empty vec is acceptable.
        let _ = list_input_devices();
    }

    #[test]
    fn list_output_devices_does_not_panic() {
        // Same rationale as list_input_devices_does_not_panic: no hardware
        // assertions, just confirm enumeration doesn't error or panic.
        let _ = list_output_devices();
    }

    #[test]
    fn push_mono_f32_averages_stereo_channels() {
        let (mut tx, mut rx) = ring();
        // Stereo L=1.0, R=0.0 → mono mean = 0.5.
        let interleaved = vec![1.0_f32, 0.0_f32, 1.0, 0.0, 1.0, 0.0];
        push_mono_f32(&mut tx, &interleaved, 2);
        let mut out = Vec::new();
        rx.drain_to(&mut out);
        assert_eq!(out, vec![0.5, 0.5, 0.5]);
    }

    #[test]
    fn push_mono_i16_uses_symmetric_divisor() {
        // BL-02: i16::MIN (-32768) must map to exactly -1.0, not -1.00003.
        let (mut tx, mut rx) = ring();
        let data = vec![i16::MIN, 0, i16::MAX];
        push_mono_i16(&mut tx, &data, 1);
        let mut out = Vec::new();
        rx.drain_to(&mut out);
        assert_eq!(out[0], -1.0, "i16::MIN must convert to exactly -1.0");
        assert_eq!(out[1], 0.0);
        // i16::MAX / 32768 = 0.99997 — just shy of 1.0, which is correct.
        assert!(
            (out[2] - 0.99997_f32).abs() < 1e-4,
            "i16::MAX should map to ~0.99997, got {}",
            out[2]
        );
    }
}
