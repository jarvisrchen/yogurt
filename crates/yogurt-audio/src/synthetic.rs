//! Synthetic sine-wave PCM generator for exercising broadcast plumbing without real audio devices.

use crate::{Channel, Frame, FRAME_SAMPLES, SAMPLE_RATE_HZ};
use std::time::Duration;
use tokio::{sync::broadcast, task::JoinHandle, time::Instant};

#[derive(Debug, Clone)]
pub struct SineWaveConfig {
    pub channel: Channel,
    pub frequency_hz: f32,
    pub amplitude: i16,
}

impl SineWaveConfig {
    /// 440 Hz at about half full scale.
    pub fn default_for(channel: Channel) -> Self {
        Self {
            channel,
            frequency_hz: 440.0,
            amplitude: 16_000,
        }
    }
}

/// Emits a sine-wave frame every 20 ms until the returned handle is aborted.
/// `frame_idx` derives from elapsed time, so the phase stays locked to `monotonic_micros` across missed ticks.
pub fn spawn_sine_wave(cfg: SineWaveConfig, tx: broadcast::Sender<Frame>) -> JoinHandle<()> {
    tokio::spawn(async move {
        let start = Instant::now();
        let mut interval = tokio::time::interval(Duration::from_millis(20));
        // Drop late ticks instead of firing back-to-back to catch up.
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            interval.tick().await;
            let elapsed = start.elapsed();
            let monotonic_micros = elapsed.as_micros() as u64;
            let frame_idx = monotonic_micros / 20_000;
            let samples = generate_chunk(&cfg, frame_idx);
            let frame = Frame::new(cfg.channel, monotonic_micros, samples);
            let _ = tx.send(frame);
        }
    })
}

/// Indexed from the global sample position so adjacent frames join without a phase click.
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
        let cfg = SineWaveConfig::default_for(Channel::Mic);
        let chunk_a = generate_chunk(&cfg, 0);
        let chunk_b = generate_chunk(&cfg, 1);
        let last_of_a = chunk_a[FRAME_SAMPLES - 1] as i32;
        let first_of_b = chunk_b[0] as i32;
        assert!(
            (last_of_a - first_of_b).abs() < 5_000,
            "phase discontinuity: last_of_a={last_of_a}, first_of_b={first_of_b}"
        );
    }
}
