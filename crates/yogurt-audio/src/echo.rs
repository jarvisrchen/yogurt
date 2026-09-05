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

/// Resolve an output device and a usable stream config. Honors
/// `preferred_rate` when the device supports it (so a matching rate needs no
/// resampling), otherwise falls back to the device's own default config. A
/// `preferred_rate` of 0 always takes the default. Returns the device, its
/// name, the fixed-buffer config, its sample format, and the actual rate.
fn choose_output_config(
    device_name: Option<&str>,
    buffer: u32,
    preferred_rate: u32,
) -> Result<(
    cpal::Device,
    String,
    cpal::StreamConfig,
    cpal::SampleFormat,
    u32,
)> {
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

    let chosen = device
        .supported_output_configs()
        .map_err(|e| AudioError::Cpal(format!("supported_output_configs: {e}")))?
        .find(|c| {
            c.min_sample_rate().0 <= preferred_rate && preferred_rate <= c.max_sample_rate().0
        })
        .map(|c| c.with_sample_rate(cpal::SampleRate(preferred_rate)))
        .or_else(|| device.default_output_config().ok())
        .ok_or_else(|| AudioError::OutputUnavailable("no usable output config".into()))?;

    let actual_rate = chosen.sample_rate().0;
    let sample_format = chosen.sample_format();
    let mut config: cpal::StreamConfig = chosen.into();
    config.buffer_size = cpal::BufferSize::Fixed(buffer);

    Ok((device, resolved_name, config, sample_format, actual_rate))
}

/// Streaming linear resampler over a mono `f32` source (which yields 0.0 on
/// underrun or while muted). Allocation-free and lock-free, so it runs
/// inside the output callback.
///
/// ponytail: linear interpolation with no anti-alias filter. Fine for a
/// live mic monitor into Zoom/OBS, which resample again downstream; the
/// upgrade path if fidelity ever matters is resample.rs's rubato.
struct LinearResampler<F> {
    pull: F,
    /// Source samples advanced per output sample (`src_rate / dst_rate`).
    step: f64,
    /// Fractional position between `a` and `b`, in `[0, 1)` before `next`.
    t: f64,
    a: f32,
    b: f32,
    primed: bool,
}

impl<F: FnMut() -> f32> LinearResampler<F> {
    fn new(pull: F, src_rate: u32, dst_rate: u32) -> Self {
        Self {
            pull,
            step: src_rate as f64 / dst_rate as f64,
            t: 0.0,
            a: 0.0,
            b: 0.0,
            primed: false,
        }
    }

    fn next(&mut self) -> f32 {
        if !self.primed {
            self.a = (self.pull)();
            self.b = (self.pull)();
            self.primed = true;
        }
        while self.t >= 1.0 {
            self.a = self.b;
            self.b = (self.pull)();
            self.t -= 1.0;
        }
        let out = self.a + (self.b - self.a) * self.t as f32;
        self.t += self.step;
        out
    }
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
    let (_, _, _, _, rate) = choose_output_config(device, buffer, 0)?;
    let mut phase = 0.0_f32;
    let echo = MicEcho::start_with_source(device, buffer, 0, move || {
        Some(tone_sample(&mut phase, rate))
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
    /// output) that plays back `consumer`'s mono sample stream, resampling
    /// from `mic_rate` to whatever rate the device supports.
    pub(crate) fn start(
        device_name: Option<&str>,
        buffer: u32,
        mic_rate: u32,
        muted: Arc<AtomicBool>,
        consumer: Arc<Mutex<AudioRingConsumer>>,
    ) -> Result<Self> {
        let (_, _, _, _, dev_rate) = choose_output_config(device_name, buffer, mic_rate)?;

        let pull = move || {
            if muted.load(Ordering::Relaxed) {
                0.0
            } else {
                // ponytail: try_lock must never block the IOProc; a failed
                // lock zero-fills instead. Upgrade path: an arc-swappable
                // consumer if this ever shows real contention.
                consumer
                    .try_lock()
                    .ok()
                    .and_then(|mut g| g.pop())
                    .unwrap_or(0.0)
            }
        };

        let source: Box<dyn FnMut() -> Option<f32> + Send> = if dev_rate == mic_rate {
            Box::new(move || Some(pull()))
        } else {
            let mut resampler = LinearResampler::new(pull, mic_rate, dev_rate);
            Box::new(move || Some(resampler.next()))
        };

        Self::start_with_source(device_name, buffer, mic_rate, source)
    }

    /// Shared build path for both the mic-echo stream and the test-tone
    /// stream: opens `device_name`, picks a config near `preferred_rate`,
    /// and fans `next_sample`'s output to every channel. `next_sample` must
    /// produce at the resolved device rate.
    fn start_with_source(
        device_name: Option<&str>,
        buffer: u32,
        preferred_rate: u32,
        mut next_sample: impl FnMut() -> Option<f32> + Send + 'static,
    ) -> Result<Self> {
        let (device, resolved_name, config, sample_format, actual_rate) =
            choose_output_config(device_name, buffer, preferred_rate)?;
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
                return Err(AudioError::OutputUnavailable(format!(
                    "sample format {other:?} unsupported; only F32 output is wired up"
                )))
            }
        };

        stream
            .play()
            .map_err(|e| AudioError::Cpal(format!("echo stream.play(): {e}")))?;

        let latency_ms = buffer as f64 / actual_rate as f64 * 1000.0;
        tracing::info!(
            device = %resolved_name,
            sample_rate = actual_rate,
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
    fn resampler_passes_equal_rates_through_unchanged() {
        let src = [1.0_f32, 2.0, 3.0, 4.0];
        let mut it = src.into_iter();
        let mut r = LinearResampler::new(move || it.next().unwrap_or(0.0), 48_000, 48_000);
        assert_eq!(r.next(), 1.0);
        assert_eq!(r.next(), 2.0);
        assert_eq!(r.next(), 3.0);
    }

    #[test]
    fn resampler_downsamples_by_taking_fewer_output_samples() {
        // 48k -> 24k: two source samples per output sample.
        let mut n = 0.0_f32;
        let mut r = LinearResampler::new(
            move || {
                n += 1.0;
                n
            },
            48_000,
            24_000,
        );
        let a = r.next();
        let b = r.next();
        let c = r.next();
        assert!(a < b && b < c, "expected increasing {a} {b} {c}");
        assert!((b - a - 2.0).abs() < 1e-3, "step should be ~2 samples");
    }

    #[test]
    fn resampler_stays_within_input_bounds() {
        let src: Vec<f32> = (0..96).map(|i| (i as f32 * 0.13).sin()).collect();
        let mut it = src.clone().into_iter();
        let mut r = LinearResampler::new(move || it.next().unwrap_or(0.0), 48_000, 44_100);
        let hi = src.iter().cloned().fold(f32::MIN, f32::max);
        let lo = src.iter().cloned().fold(f32::MAX, f32::min);
        for _ in 0..80 {
            let s = r.next();
            assert!(s <= hi + 1e-6 && s >= lo - 1e-6, "{s} out of input bounds");
        }
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
    fn unsupported_preferred_rate_falls_back_instead_of_erroring() {
        // The bug the user hit: a Bluetooth output whose supported range
        // excludes 48 kHz. Simulate with a rate no device advertises and
        // assert we fall back to the device's own rate rather than
        // returning OutputUnavailable. Skips with no output device.
        let host = cpal::default_host();
        let Some(device) = host.default_output_device() else {
            return;
        };
        let name = device.name().ok();
        let res = choose_output_config(name.as_deref(), 512, 999_999);
        let (_, _, _, _, rate) = res.expect("must fall back, not error");
        assert_ne!(rate, 999_999, "should have used a real device rate");
        assert!(rate >= 8_000, "implausible fallback rate {rate}");
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
