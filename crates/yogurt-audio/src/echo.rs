//! AUD-11: echo the mic's mono sample stream out to a chosen cpal output
//! device (typically a virtual device like "BlackHole 2ch") so other apps
//! (Zoom, OBS) can consume the mic without yogurt being in the call.
//!
//! Audio never leaves the process - the output stream reads from the same
//! SPSC ring the mic's input callback tees into (see `mic.rs`'s `TeeSink`).
//! Echo failure is always non-fatal to recording: every entry point here
//! returns a `Result` the caller logs and falls back from, never a panic
//! or a failure that stops capture.

use crate::error::{AudioError, Result};
use crate::ring::AudioRingConsumer;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Live echo output stream. Drop to stop it (RAII, same pattern as
/// `MicCapture`).
pub struct MicEcho {
    _stream: cpal::Stream,
    device_name: String,
}

impl std::fmt::Debug for MicEcho {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MicEcho")
            .field("device_name", &self.device_name)
            .finish_non_exhaustive()
    }
}

impl MicEcho {
    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    /// Open an output stream on `device_name` (`None`/`""` = system default
    /// output) that plays back `consumer`'s mono sample stream, fanned out
    /// to every output channel, zero-filled on underrun or while `muted`.
    ///
    /// `sample_rate` must equal the mic's native rate - the output config is
    /// pinned to it exactly.
    ///
    /// ponytail: requires an exact sample-rate match (or a device whose
    /// supported range covers it) rather than resampling; upgrade path is
    /// wiring `resample.rs`'s rubato path in here if a real device's output
    /// range never covers the mic's rate.
    pub(crate) fn start(
        device_name: Option<&str>,
        buffer: u32,
        sample_rate: u32,
        muted: Arc<AtomicBool>,
        consumer: Arc<Mutex<AudioRingConsumer>>,
    ) -> Result<Self> {
        let host = cpal::default_host();
        let device = match device_name {
            Some(name) if !name.is_empty() => host
                .output_devices()
                .map_err(|e| AudioError::Cpal(format!("output_devices(): {e}")))?
                .find(|d| d.name().map(|n| n == name).unwrap_or(false))
                .ok_or_else(|| AudioError::OutputUnavailable(format!("not found: {name}")))?,
            _ => host
                .default_output_device()
                .ok_or_else(|| AudioError::OutputUnavailable("no default output device".into()))?,
        };
        let resolved_name = device.name().unwrap_or_else(|_| "<unknown>".to_string());

        let supported_range = device
            .supported_output_configs()
            .map_err(|e| AudioError::Cpal(format!("supported_output_configs: {e}")))?
            .find(|c| c.min_sample_rate().0 <= sample_rate && sample_rate <= c.max_sample_rate().0)
            .ok_or_else(|| {
                AudioError::OutputUnavailable(format!("does not support {sample_rate} Hz"))
            })?;
        let supported = supported_range.with_sample_rate(cpal::SampleRate(sample_rate));
        let channels = supported.channels() as usize;
        let sample_format = supported.sample_format();
        let mut config: cpal::StreamConfig = supported.into();
        config.buffer_size = cpal::BufferSize::Fixed(buffer);

        let err_callback = |e: cpal::StreamError| {
            tracing::error!(error = %e, "cpal echo output stream error");
        };

        let stream = match sample_format {
            cpal::SampleFormat::F32 => device
                .build_output_stream::<f32, _, _>(
                    &config,
                    move |out: &mut [f32], _: &cpal::OutputCallbackInfo| {
                        let is_muted = muted.load(Ordering::Relaxed);
                        // ponytail: try_lock, not a blocking lock. Nothing
                        // else ever touches this consumer while a stream is
                        // live - `AudioStream` always drops the previous
                        // `MicEcho` before installing a new one - so
                        // contention should never actually happen. Treat a
                        // failed lock as underrun (zero-fill) rather than
                        // blocking the CoreAudio IOProc, which must not
                        // stall. Upgrade path: an arc-swappable consumer if
                        // this ever shows real contention.
                        let Ok(mut guard) = consumer.try_lock() else {
                            out.fill(0.0);
                            return;
                        };
                        fan_out(out, channels, is_muted, || guard.pop());
                    },
                    err_callback,
                    None,
                )
                .map_err(|e| AudioError::Cpal(format!("build_output_stream f32: {e}")))?,
            other => {
                return Err(AudioError::Cpal(format!(
                    "unsupported echo output sample format {other:?}; expected F32"
                )))
            }
        };

        stream
            .play()
            .map_err(|e| AudioError::Cpal(format!("echo stream.play(): {e}")))?;

        let latency_ms = buffer as f64 / sample_rate as f64 * 1000.0;
        tracing::info!(
            device = %resolved_name,
            sample_rate,
            buffer,
            latency_ms,
            "mic echo started"
        );

        Ok(Self {
            _stream: stream,
            device_name: resolved_name,
        })
    }
}

impl Drop for MicEcho {
    fn drop(&mut self) {
        // AUD-11 E2E fix (the real root cause of the "tone never stops"
        // bug): cpal 0.15's macOS `Stream` registers a device-disconnect
        // listener that clones the `Stream` into its own closure, which
        // that closure stores back inside the very `StreamInner` it points
        // to (`add_disconnect_listener` in cpal's coreaudio backend) - a
        // permanent `Arc` self-reference cycle. The refcount never reaches
        // zero, so the underlying `AudioUnit` never actually disposes on
        // Drop alone; the render callback would keep running forever.
        // `pause()` sidesteps this: it calls `audio_unit.stop()` directly
        // through the shared Mutex regardless of the leaked Arc clone, so
        // it's the only thing that reliably silences the stream. Not a
        // cpal version we can bump our way out of at time of writing —
        // upgrade path is dropping this workaround if a future cpal fixes
        // the listener's own lifetime.
        let _ = self._stream.pause();
        tracing::info!(device = %self.device_name, "mic echo stopped");
    }
}

/// Fan a mono sample stream out to `channels` interleaved output channels.
/// Zero-fills every frame while `muted`, and zero-fills any frame
/// `pop_one` can't satisfy (ring underrun) rather than repeating stale
/// audio. Pure function over slices/closures - no cpal, no ring type - so
/// it's unit-testable without a real device.
fn fan_out(
    out: &mut [f32],
    channels: usize,
    muted: bool,
    mut pop_one: impl FnMut() -> Option<f32>,
) {
    if channels == 0 {
        return;
    }
    for frame in out.chunks_mut(channels) {
        let sample = if muted { 0.0 } else { pop_one().unwrap_or(0.0) };
        for s in frame.iter_mut() {
            *s = sample;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fans_one_mono_sample_across_all_channels() {
        let samples = [1.0_f32, 2.0, 3.0];
        let mut it = samples.into_iter();
        let mut out = [0.0_f32; 6]; // 2 frames x 3 channels
        fan_out(&mut out, 3, false, || it.next());
        assert_eq!(out, [1.0, 1.0, 1.0, 2.0, 2.0, 2.0]);
    }

    #[test]
    fn zero_fills_on_underrun() {
        let mut it = std::iter::once(1.0_f32);
        let mut out = [9.0_f32; 4]; // 2 frames x 2 channels, only 1 sample available
        fan_out(&mut out, 2, false, || it.next());
        assert_eq!(out, [1.0, 1.0, 0.0, 0.0]);
    }

    #[test]
    fn zero_fills_entirely_while_muted() {
        let mut it = [1.0_f32, 2.0].into_iter();
        let mut out = [9.0_f32; 4];
        fan_out(&mut out, 2, true, || it.next());
        assert_eq!(out, [0.0; 4]);
        // Muted never even touches the source.
        assert_eq!(it.next(), Some(1.0));
    }

    #[test]
    fn zero_channels_is_a_no_op() {
        let mut out = [9.0_f32; 2];
        fan_out(&mut out, 0, false, || Some(1.0));
        assert_eq!(out, [9.0, 9.0]);
    }

    #[test]
    fn list_output_devices_does_not_panic() {
        let _ = crate::list_output_devices();
    }

    /// AUD-11 E2E regression: real hardware showed echo output streams
    /// stacking up across toggles/hot-swaps instead of the old one being
    /// dropped. The fix is ordering (`AudioStream::open_echo`/`stop_echo`
    /// drop the previous `MicEcho` before/instead of building a new one)
    /// rather than anything cpal-specific, so this mirrors the exact
    /// `Option<T>` replace/clear pattern those methods use and proves it
    /// drops synchronously - no hardware needed.
    #[test]
    fn replacing_or_clearing_an_option_drops_the_previous_value_immediately() {
        use std::sync::atomic::{AtomicUsize, Ordering as AtoOrdering};

        struct DropCounter<'a>(&'a AtomicUsize);
        impl Drop for DropCounter<'_> {
            fn drop(&mut self) {
                self.0.fetch_add(1, AtoOrdering::SeqCst);
            }
        }

        let drops = AtomicUsize::new(0);

        // Mirrors `stop_echo`: clearing the slot must drop synchronously.
        {
            let mut slot = Some(DropCounter(&drops));
            clear(&mut slot);
            assert_eq!(drops.load(AtoOrdering::SeqCst), 1);
        }

        // Mirrors `open_echo`: replacing Some(_) with Some(_) must drop the
        // old value before the new one is stored, not after, not never.
        {
            let mut slot = Some(DropCounter(&drops));
            replace(&mut slot, DropCounter(&drops));
            assert_eq!(
                drops.load(AtoOrdering::SeqCst),
                2,
                "replacing Some(_) must drop the previous value"
            );
            drop(slot);
            assert_eq!(drops.load(AtoOrdering::SeqCst), 3);
        }

        // Opaque functions so the compiler can't "helpfully" flag the
        // reassignment inside as a dead store - it's the whole point.
        fn clear<T>(slot: &mut Option<T>) {
            *slot = None;
        }
        fn replace<T>(slot: &mut Option<T>, new: T) {
            *slot = Some(new);
        }
    }
}
