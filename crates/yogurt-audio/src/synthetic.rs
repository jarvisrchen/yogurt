//! Synthetic sine-wave PCM generator. Used by tests to exercise broadcast
//! plumbing without touching real audio devices, and exposed via the
//! `synthetic` feature so Phase 5 can offer it as a debug input source.

use crate::{Channel, Frame, FRAME_SAMPLES, SAMPLE_RATE_HZ};
use std::time::Duration;
use tokio::{sync::broadcast, task::JoinHandle, time::Instant};

/// Configuration for [`spawn_sine_wave`].
///
/// The defaults from [`SineWaveConfig::default_for`] (440 Hz, amplitude
/// 16_000 ≈ half full-scale i16) are loud enough to be obviously non-zero
/// in tests but well below clipping.
#[derive(Debug, Clone)]
pub struct SineWaveConfig {
    pub channel: Channel,
    pub frequency_hz: f32,
    pub amplitude: i16,
}

impl SineWaveConfig {
    /// Sensible default: 440 Hz, amplitude 16_000 (~ half full-scale i16).
    pub fn default_for(channel: Channel) -> Self {
        Self {
            channel,
            frequency_hz: 440.0,
            amplitude: 16_000,
        }
    }
}

/// Spawn a task that emits sine-wave frames at the real 20 ms cadence over
/// the supplied `broadcast::Sender<Frame>`. The returned [`JoinHandle`] can
/// be `.abort()`-ed to stop generation; on drop without abort, the task
/// runs until the runtime shuts down.
///
/// **Send semantics:** if there are no live receivers, `Sender::send`
/// returns `Err` and we drop the frame silently — matches real producer
/// behaviour when consumers fall behind (D-19).
///
/// **WR-06 (synthetic desync):** `frame_idx` is computed from elapsed
/// wall-clock time rather than `frame_idx += 1` on each tick. With
/// `MissedTickBehavior::Delay`, the previous code emitted the *next*
/// `frame_idx` regardless of how long the tokio scheduler stalled — so a
/// 100 ms scheduler delay produced a frame whose sine phase corresponded
/// to `N+1 * 20 ms` but whose `monotonic_micros` was already `N + 100 ms`.
/// Deriving `frame_idx` from elapsed time keeps the sine wave's phase
/// locked to wall-clock time even across missed ticks (the phase will
/// jump rather than smear — but it will not lie about which point in
/// time the samples correspond to).
pub fn spawn_sine_wave(cfg: SineWaveConfig, tx: broadcast::Sender<Frame>) -> JoinHandle<()> {
    tokio::spawn(async move {
        let start = Instant::now();
        let mut interval = tokio::time::interval(Duration::from_millis(20));
        // Avoid catching up after a slow consumer — drop late ticks instead
        // of firing back-to-back to make up lost time. Matches D-19's
        // "drop late frames" behaviour.
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            interval.tick().await;
            let elapsed = start.elapsed();
            let monotonic_micros = elapsed.as_micros() as u64;
            // WR-06: derive frame_idx from elapsed time so the sine wave's
            // phase stays consistent with monotonic_micros even after a
            // missed tick. 20 ms = 20_000 µs.
            let frame_idx = monotonic_micros / 20_000;
            let samples = generate_chunk(&cfg, frame_idx);
            let frame = Frame::new(cfg.channel, monotonic_micros, samples);
            // No live receivers → `send` returns Err → drop silently.
            let _ = tx.send(frame);
        }
    })
}

/// Produce one `FRAME_SAMPLES`-length chunk of a sine wave, indexed from a
/// **global** sample position so adjacent frames join without phase
/// discontinuities (no audible click between frames).
fn generate_chunk(cfg: &SineWaveConfig, frame_idx: u64) -> Vec<i16> {
    let sr = SAMPLE_RATE_HZ as f32;
    let two_pi_f = 2.0 * std::f32::consts::PI * cfg.frequency_hz;
    let base_sample = frame_idx * FRAME_SAMPLES as u64;
    (0..FRAME_SAMPLES)
        .map(|i| {
            let t = (base_sample + i as u64) as f32 / sr;
            (cfg.amplitude as f32 * (two_pi_f * t).sin()) as i16
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_generates_frame_sized_chunks() {
        let cfg = SineWaveConfig::default_for(Channel::Mic);
        let chunk = generate_chunk(&cfg, 0);
        assert_eq!(chunk.len(), FRAME_SAMPLES);
    }

    #[test]
    fn it_produces_continuous_phase_across_frames() {
        // The last sample of frame N and the first of frame N+1 should differ
        // by roughly one sample-period of the sine wave — proves we're using
        // global sample index, not per-frame index (which would click on
        // every 20 ms boundary).
        let cfg = SineWaveConfig::default_for(Channel::Mic);
        let chunk_a = generate_chunk(&cfg, 0);
        let chunk_b = generate_chunk(&cfg, 1);
        let last_of_a = chunk_a[FRAME_SAMPLES - 1] as i32;
        let first_of_b = chunk_b[0] as i32;
        // 440 Hz at 16 kHz = ~36 samples per cycle, peak-to-peak amplitude
        // ~32 000. Adjacent samples differ by at most ~2 800.
        assert!(
            (last_of_a - first_of_b).abs() < 5_000,
            "phase discontinuity: last_of_a={last_of_a}, first_of_b={first_of_b}"
        );
    }
}
