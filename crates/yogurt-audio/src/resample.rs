//! Downmix to mono and resample to 16 kHz i16, shared by the mic and system paths.
//! Samples accumulate until one `INPUT_CHUNK` is ready, then run through `rubato::SincFixedIn`.

use crate::{AudioError, Result};
use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};

/// 480 samples @ 48 kHz = 10 ms, about 160 output samples @ 16 kHz.
const INPUT_CHUNK: usize = 480;

pub struct Downmix {
    input_channels: u16,
    #[allow(dead_code)]
    input_rate: u32,
    resampler: SincFixedIn<f32>,
    pending_mono: Vec<f32>,
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
    pub fn new(input_rate: u32, input_channels: u16) -> Result<Self> {
        let params = SincInterpolationParameters {
            sinc_len: 64,
            f_cutoff: 0.95,
            interpolation: SincInterpolationType::Linear,
            oversampling_factor: 128,
            window: WindowFunction::BlackmanHarris2,
        };
        let ratio = 16_000.0 / input_rate as f64;
        let resampler =
            SincFixedIn::<f32>::new(ratio, 1.0, params, INPUT_CHUNK, 1).map_err(|e| {
                AudioError::Cpal(format!("rubato SincFixedIn construction failed: {e}"))
            })?;
        // Sized to the upper bound so the hot path never allocates.
        let scratch_out = vec![vec![0.0_f32; resampler.output_frames_max()]];
        Ok(Self {
            input_channels,
            input_rate,
            resampler,
            pending_mono: Vec::with_capacity(INPUT_CHUNK * 2),
            scratch_out,
        })
    }

    /// Returns empty when the input was smaller than one `INPUT_CHUNK`.
    pub fn push(&mut self, interleaved: &[f32]) -> Vec<i16> {
        if self.input_channels == 0 || interleaved.is_empty() {
            return Vec::new();
        }
        let ch = self.input_channels as usize;
        let frames = interleaved.len() / ch;

        self.pending_mono.reserve(frames);
        if ch == 1 {
            self.pending_mono.extend_from_slice(interleaved);
        } else {
            for frame in interleaved.chunks_exact(ch) {
                let sum: f32 = frame.iter().sum();
                self.pending_mono.push(sum / ch as f32);
            }
        }

        let mut out_i16: Vec<i16> = Vec::with_capacity(self.pending_mono.len() / 3 + 4);
        while self.pending_mono.len() >= INPUT_CHUNK {
            let chunk_in: Vec<f32> = self.pending_mono.drain(..INPUT_CHUNK).collect();
            let wave_in = [chunk_in.as_slice()];
            match self
                .resampler
                .process_into_buffer(&wave_in, &mut self.scratch_out, None)
            {
                Ok((_consumed, out_len)) => {
                    for &s in &self.scratch_out[0][..out_len] {
                        out_i16.push(f32_to_i16(s));
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

/// Multiplies by 32768 to match the divisor in `mic.rs::push_mono_i16`, so the round-trip is identity.
/// `i16::MAX` (32767) would be asymmetric with `i16::MIN`.
#[inline]
pub(crate) fn f32_to_i16(s: f32) -> i16 {
    const MAX_F32: f32 = 1.0 - 1.0 / 32768.0;
    let clamped = s.clamp(-1.0, MAX_F32);
    (clamped * 32768.0) as i16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_constructs_for_common_rates() {
        Downmix::new(48_000, 2).unwrap();
        Downmix::new(48_000, 1).unwrap();
        Downmix::new(44_100, 2).unwrap();
    }

    #[test]
    fn it_downmixes_stereo_to_mono() {
        let mut dx = Downmix::new(48_000, 2).expect("construct");
        let mut interleaved = Vec::with_capacity(INPUT_CHUNK * 2);
        for _ in 0..INPUT_CHUNK {
            interleaved.push(0.5_f32);
            interleaved.push(-0.5_f32);
        }
        let out = dx.push(&interleaved);
        assert!(!out.is_empty(), "expected resampled output, got empty");
        // The sinc delay line starts zeroed, so only check samples past the warmup.
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
        // Tolerance covers delay-line warm-up and the partial chunk.
        assert!(
            (290..=350).contains(&total),
            "expected ~320 output samples for 960 input @ ratio 1/3, got {total} (out1={}, out2={})",
            out1.len(),
            out2.len()
        );
    }

    #[test]
    fn f32_to_i16_round_trip_is_identity_within_i16_range() {
        const I16_TO_F32: f32 = 1.0 / 32768.0;
        for &s in &[i16::MIN, -32767, -1, 0, 1, 32766, i16::MAX] {
            let as_f32 = (s as f32) * I16_TO_F32;
            let back = f32_to_i16(as_f32);
            assert_eq!(
                back, s,
                "round-trip failed: i16={s} → f32={as_f32} → i16={back}"
            );
        }
    }

    #[test]
    fn f32_to_i16_clamps_out_of_range() {
        assert_eq!(f32_to_i16(2.0), i16::MAX);
        assert_eq!(f32_to_i16(-2.0), i16::MIN);
        assert_eq!(f32_to_i16(f32::INFINITY), i16::MAX);
        assert_eq!(f32_to_i16(f32::NEG_INFINITY), i16::MIN);
    }

    #[test]
    fn it_handles_under_chunk_input_by_buffering() {
        let mut dx = Downmix::new(48_000, 2).expect("construct");
        let interleaved: Vec<f32> = vec![0.1_f32; 400];
        let out = dx.push(&interleaved);
        assert!(
            out.is_empty(),
            "should buffer under-chunk input, got {} samples",
            out.len()
        );

        let more: Vec<f32> = vec![0.1_f32; 960];
        let out2 = dx.push(&more);
        assert!(!out2.is_empty(), "expected output after buffer fills");
    }
}
