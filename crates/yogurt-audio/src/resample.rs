//! Shared 48 kHz stereo f32 → 16 kHz mono i16 downmix + resample helper.
//!
//! Both the mic path (`cpal` default input typically 48 kHz f32) and the
//! system path (SCK 8.x emits 48 kHz f32 stereo by default) feed this helper
//! to produce 16 kHz mono i16 PCM that matches the [`crate::Frame`] contract.
//!
//! ## Pipeline
//!
//! 1. Downmix N input channels to mono via L+R+…/N mean.
//! 2. Accumulate mono samples in `pending_mono` until we have at least one
//!    `INPUT_CHUNK` (480 samples @ 48 kHz = 10 ms).
//! 3. Run `rubato::SincFixedIn` on each 480-sample chunk; produces ~160
//!    samples per chunk at 16 kHz target.
//! 4. Convert f32 → i16 by `(clamp(-1, 1) * i16::MAX as f32) as i16`.
//!
//! Returned `Vec<i16>` is then fed into [`crate::mic::FrameChunker`] (or the
//! system-side equivalent) which collects 320-sample (20 ms) `Frame`s.
//!
//! ## Tuning (D-14)
//!
//! `SincInterpolationParameters { sinc_len: 64, f_cutoff: 0.95, interpolation:
//! Linear, oversampling_factor: 128, window: BlackmanHarris2 }` — the same
//! parameters used by the project's reference downsample. Re-tunable if mic
//! smoke reports audible artifacts.

use crate::{AudioError, Result};
use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};

/// Number of mono input samples per `rubato` chunk. 480 @ 48 kHz = 10 ms.
/// Produces ~160 samples @ 16 kHz per call.
const INPUT_CHUNK: usize = 480;

/// Downmix + resample helper. Owns the per-stream rubato state and a buffer
/// of leftover mono samples between calls so producers can push arbitrary-
/// sized callback slices without worrying about chunk alignment.
pub struct Downmix {
    input_channels: u16,
    #[allow(dead_code)]
    input_rate: u32,
    resampler: SincFixedIn<f32>,
    /// Leftover mono samples not yet consumed by a 480-sample resample chunk.
    pending_mono: Vec<f32>,
    /// Scratch buffer for `process_into_buffer` output; allocated once.
    scratch_out: Vec<Vec<f32>>,
}

impl std::fmt::Debug for Downmix {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Downmix")
            .field("input_channels", &self.input_channels)
            .field("input_rate", &self.input_rate)
            .field("pending_mono_len", &self.pending_mono.len())
            .finish_non_exhaustive()
    }
}

impl Downmix {
    /// Build a new downmix/resample helper for an input source of
    /// `input_rate` Hz with `input_channels` interleaved channels.
    pub fn new(input_rate: u32, input_channels: u16) -> Result<Self> {
        let params = SincInterpolationParameters {
            sinc_len: 64,
            f_cutoff: 0.95,
            interpolation: SincInterpolationType::Linear,
            oversampling_factor: 128,
            window: WindowFunction::BlackmanHarris2,
        };
        // Target ratio = 16 000 / input_rate. For 48 kHz → 16 kHz this is 1/3.
        let ratio = 16_000.0 / input_rate as f64;
        let resampler =
            SincFixedIn::<f32>::new(ratio, 1.0, params, INPUT_CHUNK, 1).map_err(|e| {
                AudioError::Cpal(format!("rubato SincFixedIn construction failed: {e}"))
            })?;
        // Pre-allocate output scratch large enough for one chunk's worth of
        // output at the configured ratio. `output_frames_max()` reports the
        // upper bound so allocations don't happen on the hot path.
        let scratch_out = vec![vec![0.0_f32; resampler.output_frames_max()]];
        Ok(Self {
            input_channels,
            input_rate,
            resampler,
            pending_mono: Vec::with_capacity(INPUT_CHUNK * 2),
            scratch_out,
        })
    }

    /// Push interleaved input samples (N channels per frame) and receive any
    /// resampled 16 kHz mono i16 output that became ready. May return an
    /// empty vec if the input was smaller than one `INPUT_CHUNK`.
    pub fn push(&mut self, interleaved: &[f32]) -> Vec<i16> {
        if self.input_channels == 0 || interleaved.is_empty() {
            return Vec::new();
        }
        let ch = self.input_channels as usize;
        let frames = interleaved.len() / ch;

        // Downmix to mono via channel-mean.
        self.pending_mono.reserve(frames);
        if ch == 1 {
            self.pending_mono.extend_from_slice(interleaved);
        } else {
            for frame in interleaved.chunks_exact(ch) {
                let sum: f32 = frame.iter().sum();
                self.pending_mono.push(sum / ch as f32);
            }
        }

        // Resample in INPUT_CHUNK-sized blocks.
        let mut out_i16: Vec<i16> = Vec::with_capacity(self.pending_mono.len() / 3 + 4);
        while self.pending_mono.len() >= INPUT_CHUNK {
            // SAFETY of clarity: split off exactly INPUT_CHUNK samples.
            let chunk_in: Vec<f32> = self.pending_mono.drain(..INPUT_CHUNK).collect();
            let wave_in = [chunk_in.as_slice()];
            match self
                .resampler
                .process_into_buffer(&wave_in, &mut self.scratch_out, None)
            {
                Ok((_consumed, out_len)) => {
                    for &s in &self.scratch_out[0][..out_len] {
                        let clamped = s.clamp(-1.0, 1.0);
                        out_i16.push((clamped * i16::MAX as f32) as i16);
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "rubato process_into_buffer failed; dropping chunk");
                }
            }
        }
        out_i16
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_constructs_for_common_rates() {
        // Most macs use 48k by default; cpal devices sometimes 44.1k.
        Downmix::new(48_000, 2).unwrap();
        Downmix::new(48_000, 1).unwrap();
        Downmix::new(44_100, 2).unwrap();
    }

    #[test]
    fn it_downmixes_stereo_to_mono() {
        // L = +0.5, R = -0.5 → mono mean = 0.0 → output i16 ≈ 0.
        let mut dx = Downmix::new(48_000, 2).expect("construct");
        // Feed 480 stereo frames (960 interleaved samples) so we get exactly
        // one resample chunk's worth of output.
        let mut interleaved = Vec::with_capacity(INPUT_CHUNK * 2);
        for _ in 0..INPUT_CHUNK {
            interleaved.push(0.5_f32);
            interleaved.push(-0.5_f32);
        }
        let out = dx.push(&interleaved);
        assert!(!out.is_empty(), "expected resampled output, got empty");
        // Centre samples should be ~0 (the SincFixedIn delay line is zero-
        // initialised so the very first samples are ramped from the zero
        // baseline — check well-past-the-edge samples).
        for (i, &s) in out.iter().enumerate().skip(out.len() / 2) {
            assert!(
                s.abs() < 800,
                "post-warmup sample {i} should be near zero, got {s}"
            );
        }
    }

    #[test]
    fn it_resamples_48k_to_16k_ratio() {
        // Feed exactly one 480-sample mono input chunk; the SincFixedIn warm-
        // up causes the very first call to emit fewer samples than steady-
        // state (we observe 148 on the first call, ~160 thereafter). Two
        // chunks should produce ≥ ~300 samples total — well within
        // expectation for a 1/3 ratio.
        let mut dx = Downmix::new(48_000, 1).expect("construct");
        let interleaved: Vec<f32> = (0..INPUT_CHUNK).map(|i| (i as f32).sin() * 0.1).collect();
        let out1 = dx.push(&interleaved);
        let out2 = dx.push(&interleaved);
        let total = out1.len() + out2.len();
        // 480 + 480 = 960 input samples at ratio 1/3 → ~320 output samples;
        // tolerate ±30 to account for delay-line warm-up and partial chunk.
        assert!(
            (290..=350).contains(&total),
            "expected ~320 output samples for 960 input @ ratio 1/3, got {total} (out1={}, out2={})",
            out1.len(),
            out2.len()
        );
    }

    #[test]
    fn it_handles_under_chunk_input_by_buffering() {
        let mut dx = Downmix::new(48_000, 2).expect("construct");
        // Feed less than one chunk (200 stereo frames = 400 interleaved < 960).
        let interleaved: Vec<f32> = vec![0.1_f32; 400];
        let out = dx.push(&interleaved);
        assert!(
            out.is_empty(),
            "should buffer under-chunk input, got {} samples",
            out.len()
        );

        // Top up with enough to make it past INPUT_CHUNK total.
        let more: Vec<f32> = vec![0.1_f32; 960];
        let out2 = dx.push(&more);
        assert!(!out2.is_empty(), "expected output after buffer fills");
    }
}
