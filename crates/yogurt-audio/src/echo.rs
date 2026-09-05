//! Echoes the mic's mono sample stream out to a chosen cpal output device.
//! Audio stays in-process: the output stream reads from the same ring the
//! mic's input callback tees into.

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

/// Resolve an output device, pick its `sample_rate`-capable config, and
/// build the fixed-size `StreamConfig` both `MicEcho::start` and
/// `play_test_tone` need. Shared so device lookup + config selection is
/// written once.
fn open_output_device(
    device_name: Option<&str>,
    buffer: u32,
    sample_rate: u32,
) -> Result<(cpal::Device, String, cpal::StreamConfig, cpal::SampleFormat)> {
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
    let sample_format = supported.sample_format();
    let mut config: cpal::StreamConfig = supported.into();
    config.buffer_size = cpal::BufferSize::Fixed(buffer);

    Ok((device, resolved_name, config, sample_format))
}

/// Next sample of a 440 Hz sine at amplitude 0.2, advancing `phase` by one
/// sample at `sample_rate`. `phase` wraps at `2*PI` so it never grows
/// unbounded across a long-running callback.
fn tone_sample(phase: &mut f32, sample_rate: u32) -> f32 {
    const FREQ_HZ: f32 = 440.0;
    const AMPLITUDE: f32 = 0.2;
    let sample = AMPLITUDE * phase.sin();
    *phase += 2.0 * std::f32::consts::PI * FREQ_HZ / sample_rate as f32;
    if *phase >= 2.0 * std::f32::consts::PI {
        *phase -= 2.0 * std::f32::consts::PI;
    }
    sample
}

/// Play a 440 Hz test tone on `device` (`None`/`""` = system default output)
/// for 700 ms, at the device's own default sample rate. Blocks the calling
/// thread; callers run it via `spawn_blocking`. Returns the resolved device
/// name.
pub fn play_test_tone(device: Option<&str>, buffer: u32) -> Result<String> {
    let host = cpal::default_host();
    let probe = match device {
        Some(name) if !name.is_empty() => host
            .output_devices()
            .map_err(|e| AudioError::Cpal(format!("output_devices(): {e}")))?
            .find(|d| d.name().map(|n| n == name).unwrap_or(false))
            .ok_or_else(|| AudioError::OutputUnavailable(format!("not found: {name}")))?,
        _ => host
            .default_output_device()
            .ok_or_else(|| AudioError::OutputUnavailable("no default output device".into()))?,
    };
    let sample_rate = probe
        .default_output_config()
        .map_err(|e| AudioError::Cpal(format!("default_output_config: {e}")))?
        .sample_rate()
        .0;

    let mut phase = 0.0_f32;
    let echo = MicEcho::start_with_source(device, buffer, sample_rate, move || {
        Some(tone_sample(&mut phase, sample_rate))
    })?;
    let resolved_name = echo.device_name().to_string();
    std::thread::sleep(std::time::Duration::from_millis(700));
    drop(echo);
    Ok(resolved_name)
}

impl MicEcho {
    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    /// Open an output stream on `device_name` (`None`/`""` = system default
    /// output) that plays back `consumer`'s mono sample stream.
    ///
    /// ponytail: `sample_rate` must equal the mic's native rate exactly;
    /// upgrade path is wiring resample.rs's rubato path in here if a real
    /// device's output range never covers the mic's rate.
    pub(crate) fn start(
        device_name: Option<&str>,
        buffer: u32,
        sample_rate: u32,
        muted: Arc<AtomicBool>,
        consumer: Arc<Mutex<AudioRingConsumer>>,
    ) -> Result<Self> {
        Self::start_with_source(device_name, buffer, sample_rate, move || {
            let is_muted = muted.load(Ordering::Relaxed);
            if is_muted {
                None
            } else {
                // ponytail: try_lock must never block the IOProc; a failed
                // lock zero-fills instead. Upgrade path: an arc-swappable
                // consumer if this ever shows real contention.
                consumer.try_lock().ok().and_then(|mut g| g.pop())
            }
        })
    }

    /// Shared build path for both the mic-echo stream and the test-tone
    /// stream: opens `device_name`, picks a `sample_rate`-capable F32
    /// config, and fans `next_sample`'s output to every channel.
    fn start_with_source(
        device_name: Option<&str>,
        buffer: u32,
        sample_rate: u32,
        mut next_sample: impl FnMut() -> Option<f32> + Send + 'static,
    ) -> Result<Self> {
        let (device, resolved_name, config, sample_format) =
            open_output_device(device_name, buffer, sample_rate)?;
        let channels = config.channels as usize;

        let err_callback = |e: cpal::StreamError| {
            tracing::error!(error = %e, "cpal echo output stream error");
        };

        let stream = match sample_format {
            cpal::SampleFormat::F32 => device
                .build_output_stream::<f32, _, _>(
                    &config,
                    move |out: &mut [f32], _: &cpal::OutputCallbackInfo| {
                        fan_out(out, channels, false, &mut next_sample);
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
        // cpal 0.15's macOS Stream has a disconnect listener that clones
        // itself into its own StreamInner, a permanent Arc self-reference
        // that keeps the refcount from ever reaching zero, so Drop alone
        // never disposes the AudioUnit. pause() calls audio_unit.stop()
        // directly and reliably silences it regardless.
        let _ = self._stream.pause();
        tracing::info!(device = %self.device_name, "mic echo stopped");
    }
}

/// Fan a mono sample stream out to `channels` interleaved output channels.
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
        assert_eq!(it.next(), Some(1.0));
    }

    #[test]
    fn zero_channels_is_a_no_op() {
        let mut out = [9.0_f32; 2];
        fan_out(&mut out, 0, false, || Some(1.0));
        assert_eq!(out, [9.0, 9.0]);
    }

    #[test]
    fn tone_sample_stays_within_amplitude_and_matches_known_phases() {
        let sample_rate = 48_000;
        let mut phase = 0.0_f32;
        let first = tone_sample(&mut phase, sample_rate);
        assert!(
            (first - 0.0).abs() < 1e-4,
            "phase 0 should be ~0, got {first}"
        );

        // One quarter period in: sin(pi/2) * 0.2 == 0.2.
        let quarter_samples = sample_rate / 440 / 4;
        let mut phase = 0.0_f32;
        let mut last = 0.0;
        for _ in 0..quarter_samples {
            last = tone_sample(&mut phase, sample_rate);
        }
        assert!(
            (last - 0.2).abs() < 0.01,
            "expected ~0.2 at quarter period, got {last}"
        );

        let mut phase = 0.0_f32;
        for _ in 0..sample_rate {
            let s = tone_sample(&mut phase, sample_rate);
            assert!(
                (-0.2..=0.2).contains(&s),
                "sample {s} out of amplitude bounds"
            );
        }
    }

    #[test]
    fn list_output_devices_does_not_panic() {
        let _ = crate::list_output_devices();
    }

    /// open_echo/stop_echo rely on Option assignment dropping the old stream synchronously.
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

        {
            let mut slot = Some(DropCounter(&drops));
            clear(&mut slot);
            assert_eq!(drops.load(AtoOrdering::SeqCst), 1);
        }

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

        fn clear<T>(slot: &mut Option<T>) {
            *slot = None;
        }
        fn replace<T>(slot: &mut Option<T>, new: T) {
            *slot = Some(new);
        }
    }
}
