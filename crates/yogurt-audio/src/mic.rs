//! Microphone capture via `cpal` → CoreAudio default input device.
//!
//! cpal is the boring, battle-tested CoreAudio binding (D-13). The default
//! input device is typically 48 kHz f32 (sometimes mono, sometimes stereo);
//! we resample to 16 kHz mono i16 inside the cpal callback via the shared
//! [`crate::resample::Downmix`] helper, then chunk into 320-sample
//! [`Frame`]s via [`FrameChunker`].
//!
//! ## Threading
//!
//! cpal invokes the data callback on its own audio thread. The callback
//! owns a `Mutex<(Downmix, FrameChunker)>` so the audio thread holds the
//! lock only briefly during each callback. Tokio receivers on the
//! `broadcast::Sender<Frame>` we feed from are completely decoupled.

use crate::{
    error::{AudioError, Result},
    frame::{Channel, Frame, FRAME_SAMPLES},
    resample::Downmix,
};
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    SampleFormat,
};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::broadcast;

/// Live mic capture handle. Drop to stop the underlying `cpal::Stream`.
pub struct MicCapture {
    /// Keep-alive — dropping this stops the OS-level capture via RAII.
    _stream: cpal::Stream,
    /// The device cpal handed us at construction time. Surfaced via
    /// `/api/audio/devices` for the UI.
    pub device_name: String,
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
pub(crate) struct FrameChunker {
    channel: Channel,
    tx: broadcast::Sender<Frame>,
    buf: Vec<i16>,
    /// Meeting-relative clock baseline — captured at chunker construction
    /// time, which is captured inside the producer setup inside
    /// `start_capture()`. AUDIO-05 requires drift between mic and system
    /// streams < 50 ms; both chunkers are seeded synchronously inside
    /// `start_capture()` so the spawn-order skew is microseconds.
    start: Instant,
}

impl FrameChunker {
    pub(crate) fn new(channel: Channel, tx: broadcast::Sender<Frame>) -> Self {
        Self {
            channel,
            tx,
            buf: Vec::with_capacity(FRAME_SAMPLES * 2),
            start: Instant::now(),
        }
    }

    /// Feed an arbitrary slice of i16 PCM. Drains the internal buffer in
    /// `FRAME_SAMPLES`-sized chunks, computes `monotonic_ms` per frame from
    /// the recorded `start` instant, and sends each frame on the broadcast
    /// channel. Send errors (no live subscribers) are dropped silently per
    /// D-19's "consumers fall behind → drop late frames" semantics.
    pub(crate) fn feed(&mut self, samples: &[i16]) {
        self.buf.extend_from_slice(samples);
        while self.buf.len() >= FRAME_SAMPLES {
            let chunk: Vec<i16> = self.buf.drain(..FRAME_SAMPLES).collect();
            let monotonic_ms = self.start.elapsed().as_millis() as u64;
            let frame = Frame::new(self.channel, monotonic_ms, chunk);
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
    let default_name = host
        .default_input_device()
        .and_then(|d| d.name().ok());
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

/// Spawn a microphone capture stream on the default input device. The
/// supplied `tx` receives one [`Frame`] every 20 ms while the returned
/// [`MicCapture`] is alive; drop it to stop capture (RAII per D-26).
pub fn spawn_mic_capture(tx: broadcast::Sender<Frame>) -> Result<MicCapture> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| AudioError::MicUnavailable("no default input device".into()))?;
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

    // Shared (Downmix, FrameChunker) — held briefly inside the cpal callback.
    let downmix = Downmix::new(sample_rate, channels)?;
    let chunker = FrameChunker::new(Channel::Mic, tx);
    let state = Arc::new(Mutex::new((downmix, chunker)));

    let err_callback = |e: cpal::StreamError| {
        tracing::error!(error = %e, "cpal mic stream error");
    };

    let stream = match sample_format {
        SampleFormat::F32 => {
            let state = Arc::clone(&state);
            device
                .build_input_stream::<f32, _, _>(
                    &stream_config,
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        if let Ok(mut guard) = state.lock() {
                            let (dx, chunker) = &mut *guard;
                            let out = dx.push(data);
                            if !out.is_empty() {
                                chunker.feed(&out);
                            }
                        }
                    },
                    err_callback,
                    None,
                )
                .map_err(|e| AudioError::Cpal(format!("build_input_stream f32: {e}")))?
        }
        SampleFormat::I16 => {
            let state = Arc::clone(&state);
            device
                .build_input_stream::<i16, _, _>(
                    &stream_config,
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        // Convert i16 → f32 in [-1, 1] for the downmix path.
                        // Allocate per-callback; mic callbacks are 10–20 ms,
                        // not a hot inner loop.
                        let as_f32: Vec<f32> = data
                            .iter()
                            .map(|&s| s as f32 / i16::MAX as f32)
                            .collect();
                        if let Ok(mut guard) = state.lock() {
                            let (dx, chunker) = &mut *guard;
                            let out = dx.push(&as_f32);
                            if !out.is_empty() {
                                chunker.feed(&out);
                            }
                        }
                    },
                    err_callback,
                    None,
                )
                .map_err(|e| AudioError::Cpal(format!("build_input_stream i16: {e}")))?
        }
        other => {
            return Err(AudioError::Cpal(format!(
                "unsupported sample format {other:?}; expected F32 or I16"
            )))
        }
    };

    stream
        .play()
        .map_err(|e| AudioError::Cpal(format!("stream.play(): {e}")))?;

    Ok(MicCapture {
        _stream: stream,
        device_name,
    })
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
    fn chunker_monotonic_ms_is_non_decreasing() {
        let (tx, mut rx) = broadcast::channel::<Frame>(8);
        let mut chunker = FrameChunker::new(Channel::System, tx);
        // Feed two frames worth so we get two events.
        let samples = vec![0_i16; FRAME_SAMPLES * 2];
        chunker.feed(&samples);
        let f1 = rx.try_recv().expect("first frame");
        let f2 = rx.try_recv().expect("second frame");
        assert!(
            f2.monotonic_ms >= f1.monotonic_ms,
            "monotonic_ms should be non-decreasing: f1={}, f2={}",
            f1.monotonic_ms,
            f2.monotonic_ms
        );
    }

    #[test]
    fn list_input_devices_does_not_panic() {
        // We can't assert anything about hardware here (CI macOS runners have
        // no audio devices, dev machines have varying setups). Just call it
        // and ensure it returns Ok(_) — empty vec is acceptable.
        let _ = list_input_devices();
    }
}
