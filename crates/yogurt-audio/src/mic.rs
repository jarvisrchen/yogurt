//! Microphone capture via `cpal`.
//! The cpal data callback runs on CoreAudio's real-time IOProc thread and must not block, or macOS skips the next buffer.
//! So the callback only down-mixes to mono and pushes into a lock-free SPSC ring, with no mutex, allocation, or broadcast send.
//! A tokio drainer task does the rubato resample to 16 kHz, i16 quantize, chunking, and broadcast.

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

pub struct MicCapture {
    _stream: cpal::Stream,
    drainer: JoinHandle<()>,
    pub device_name: String,
    muted: Arc<AtomicBool>,
    /// Native sample rate this capture was opened at; the echo output
    /// stream must match it exactly.
    pub(crate) sample_rate: u32,
    echo_active: Arc<AtomicBool>,
    echo_consumer: Arc<std::sync::Mutex<AudioRingConsumer>>,
}

impl Drop for MicCapture {
    fn drop(&mut self) {
        // cpal 0.15 never disposes the stream on drop; pause() is what stops it (see echo.rs).
        let _ = self._stream.pause();
        self.drainer.abort();
    }
}

impl MicCapture {
    /// The cpal stream and drainer keep running while muted, so resuming is instant.
    pub fn set_muted(&self, muted: bool) {
        self.muted.store(muted, Ordering::Relaxed);
    }

    /// Shared with the echo output stream so muting also zero-fills the echo.
    pub(crate) fn muted_handle(&self) -> Arc<AtomicBool> {
        self.muted.clone()
    }

    pub(crate) fn set_echo_active(&self, active: bool) {
        self.echo_active.store(active, Ordering::Relaxed);
    }

    pub(crate) fn echo_consumer_handle(&self) -> Arc<std::sync::Mutex<AudioRingConsumer>> {
        self.echo_consumer.clone()
    }
}

impl std::fmt::Debug for MicCapture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MicCapture")
            .field("device_name", &self.device_name)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub name: String,
    pub is_default: bool,
    /// `None` for devices with no live config, such as virtual devices.
    pub sample_rate: Option<u32>,
}

/// Collects an arbitrary-length i16 PCM stream into `FRAME_SAMPLES` frames and broadcasts them.
pub(crate) struct FrameChunker {
    channel: Channel,
    tx: broadcast::Sender<Frame>,
    buf: Vec<i16>,
    /// Both drainers are spawned synchronously inside `start_capture()`, so mic/system skew is microseconds.
    start: Instant,
    /// While `true`, `feed` still drains `buf` but skips the broadcast, so unmuting causes no timestamp jump.
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

    /// Send errors (no live subscribers) are dropped: late consumers lose frames.
    pub(crate) fn feed(&mut self, samples: &[i16]) {
        self.buf.extend_from_slice(samples);
        while self.buf.len() >= FRAME_SAMPLES {
            let chunk: Vec<i16> = self.buf.drain(..FRAME_SAMPLES).collect();
            let monotonic_micros = self.start.elapsed().as_micros() as u64;
            if self.muted.load(Ordering::Relaxed) {
                continue;
            }
            let frame = Frame::new(self.channel, monotonic_micros, chunk);
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

/// Sends one [`Frame`] every 20 ms on `tx` while the returned [`MicCapture`] is alive.
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

    let (mut producer, mut consumer) = ring();
    // Echo ring is always allocated so start_echo never touches the audio thread.
    let (mut echo_producer, echo_consumer) = ring();
    let echo_consumer = Arc::new(std::sync::Mutex::new(echo_consumer));
    let echo_active = Arc::new(AtomicBool::new(false));
    let echo_active_cb = echo_active.clone();

    let err_callback = |e: cpal::StreamError| {
        tracing::error!(error = %e, "cpal mic stream error");
    };

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

trait Sink {
    fn push_slice(&mut self, samples: &[f32]);
}

impl Sink for AudioRingProducer {
    fn push_slice(&mut self, samples: &[f32]) {
        AudioRingProducer::push_slice(self, samples)
    }
}

/// Tees every push into `main` (always) and `echo` (while `echo_active`).
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

/// Down-mixes interleaved f32 to mono (channel-mean) on the audio thread, with no allocation.
fn push_mono_f32(sink: &mut impl Sink, data: &[f32], channels: usize) {
    if channels <= 1 {
        sink.push_slice(data);
        return;
    }
    // Fixed stack buffer, flushed when full. One IOProc callback is typically 480-960 samples.
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

/// Like [`push_mono_f32`] for i16. Divides by 32768 so `i16::MIN` maps to exactly -1.0.
fn push_mono_i16(sink: &mut impl Sink, data: &[i16], channels: usize) {
    /// Matches the multiplier in resample.rs so the round-trip is identity.
    const I16_TO_F32: f32 = 1.0 / 32768.0;

    let mut mono = [0.0_f32; 1024];
    let mut written = 0usize;
    if channels <= 1 {
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
        // CI runners have no audio devices, so only check that it does not panic.
        let _ = list_input_devices();
    }

    #[test]
    fn list_output_devices_does_not_panic() {
        let _ = list_output_devices();
    }

    #[test]
    fn push_mono_f32_averages_stereo_channels() {
        let (mut tx, mut rx) = ring();
        let interleaved = vec![1.0_f32, 0.0_f32, 1.0, 0.0, 1.0, 0.0];
        push_mono_f32(&mut tx, &interleaved, 2);
        let mut out = Vec::new();
        rx.drain_to(&mut out);
        assert_eq!(out, vec![0.5, 0.5, 0.5]);
    }

    #[test]
    fn push_mono_i16_uses_symmetric_divisor() {
        let (mut tx, mut rx) = ring();
        let data = vec![i16::MIN, 0, i16::MAX];
        push_mono_i16(&mut tx, &data, 1);
        let mut out = Vec::new();
        rx.drain_to(&mut out);
        assert_eq!(out[0], -1.0, "i16::MIN must convert to exactly -1.0");
        assert_eq!(out[1], 0.0);
        assert!(
            (out[2] - 0.99997_f32).abs() < 1e-4,
            "i16::MAX should map to ~0.99997, got {}",
            out[2]
        );
    }
}
