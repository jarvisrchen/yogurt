//! Phase 2 acceptance gate — 30-second dual-channel WAV ear-test.
//!
//! This example produces the artifact the human ear-test gate consumes
//! (ROADMAP §Phase 2 success criterion #1):
//!
//! > 30 seconds of mic + system audio captured during a real
//! > YouTube/Zoom playback writes a 2-channel WAV file that passes an
//! > ear-test (both channels audible, no silence, no clipping, no
//! > channel swap) — this gates the rest of the phase.
//!
//! ## Usage
//!
//! 1. Grant Screen Recording permission to the binary (System Settings →
//!    Privacy & Security → Screen Recording). The yogurt-audio
//!    permission test or `system_smoke` example triggers the prompt
//!    once; macOS caches per-binary, so subsequent runs are silent.
//! 2. Open Spotify or YouTube and start playing audio in a separate
//!    window. Keep it playing for the full 30 seconds.
//! 3. Run:
//!
//!    ```sh
//!    cargo run -p yogurt-audio --example wav_eartest --release
//!    ```
//!
//! 4. Talk continuously into the mic AND keep system audio playing for
//!    the full 30 seconds. Count from 1 to 60, or read a paragraph
//!    aloud — any continuous narration works.
//! 5. Open `target/yogurt-audio-eartest.wav` in QuickTime / Audacity /
//!    VLC with headphones.
//! 6. Verify the checklist printed at exit:
//!    - mic on LEFT, system on RIGHT (no channel swap)
//!    - both channels audible
//!    - no silence on either channel
//!    - no clipping
//!
//! ## Output format
//!
//! - Sample rate: **16 kHz** (matches the [`FRAME_SAMPLES`] / 20 ms
//!   format contract).
//! - Bit depth: **16-bit signed LE PCM**.
//! - Channels: **2 (interleaved: left = mic, right = system)**.
//! - Duration: **30 seconds** = 480 000 frames × 2 ch × 2 bytes =
//!   ~1.92 MB of audio data + RIFF header.
//!
//! ## Time alignment
//!
//! Each `Frame` carries `monotonic_ms` — milliseconds since
//! `start_capture()` returned (D-21). Both producer chunkers are seeded
//! synchronously inside `start_capture()`, so the two clocks share a
//! common origin to within microseconds.
//!
//! For the WAV output we **concatenate each channel's frames in arrival
//! order** rather than bucketing by `monotonic_ms`. Why: `monotonic_ms`
//! is wall-clock-truncated to whole milliseconds, but each frame is
//! exactly 320 samples = 20 ms. Wall-clock jitter combined with
//! millisecond quantization makes a "place at sample = monotonic_ms * 16"
//! scheme leave ±16-sample gaps (zero-filled from the init buffer) and
//! ±16-sample overlaps between consecutive frames — audible as constant
//! crackle/static on whichever stream has noisier inter-arrival timing
//! (in practice SCK's audio dispatch queue). Arrival-order concatenation
//! is sample-exact within each channel; the two channels may drift up
//! to one frame (20 ms) relative to each other if one side drops, but
//! that's inaudible vs. the per-millisecond clicking the timestamp-
//! bucketed approach produced. The ear-test still detects channel-swap
//! correctly because both channels share a common start instant.

use std::path::PathBuf;
use std::time::Duration;

use yogurt_audio::{start_capture, Channel, Frame, FRAME_SAMPLES, SAMPLE_RATE_HZ};

const CAPTURE_SECONDS: u64 = 30;
/// Output file lives in `target/` (gitignored). Run from workspace root.
const OUTPUT_PATH: &str = "target/yogurt-audio-eartest.wav";

/// Sliding-window length for the contiguous-silence check (in samples).
/// `1 sec * 16_000 sps = 16_000 samples`.
const SILENCE_WINDOW_SAMPLES: usize = SAMPLE_RATE_HZ as usize;
/// Per-sample i16 magnitude below which we count the sample as silent.
/// 200 ≈ −44 dBFS — well under the noise floor of a real mic in a quiet
/// room but well above true zero, so we don't false-positive on tiny DC
/// offsets.
const SILENCE_THRESHOLD: i16 = 200;

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("yogurt_audio=info,wav_eartest=info")
        .init();

    println!("yogurt-audio ear-test: capturing {CAPTURE_SECONDS} seconds of dual-channel audio");
    println!("→ talk into the mic AND keep system audio playing for the full duration");
    println!();

    let stream = start_capture(None)?;
    let mut mic_rx = stream.subscribe_mic();
    let mut sys_rx = stream.subscribe_system();

    tracing::info!(
        seconds = CAPTURE_SECONDS,
        output = OUTPUT_PATH,
        "ear-test capture starting"
    );

    let deadline = tokio::time::Instant::now() + Duration::from_secs(CAPTURE_SECONDS);

    // Collect frames as we receive them. Both producers chunk in the
    // same 20 ms frame size with synchronously-seeded baselines, so
    // their monotonic_ms values are directly comparable.
    let mut mic_frames: Vec<Frame> = Vec::with_capacity((CAPTURE_SECONDS as usize) * 50 + 16);
    let mut sys_frames: Vec<Frame> = Vec::with_capacity((CAPTURE_SECONDS as usize) * 50 + 16);
    let mut mic_lagged = 0u64;
    let mut sys_lagged = 0u64;

    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        tokio::select! {
            res = mic_rx.recv() => match res {
                Ok(frame) => {
                    debug_assert_eq!(frame.channel, Channel::Mic);
                    mic_frames.push(frame);
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    mic_lagged += n;
                    tracing::warn!(channel = "mic", lagged = n, "consumer fell behind");
                }
                Err(_) => break,
            },
            res = sys_rx.recv() => match res {
                Ok(frame) => {
                    debug_assert_eq!(frame.channel, Channel::System);
                    sys_frames.push(frame);
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    sys_lagged += n;
                    tracing::warn!(channel = "system", lagged = n, "consumer fell behind");
                }
                Err(_) => break,
            },
            _ = tokio::time::sleep(remaining) => break,
        }
    }

    // Drop the live stream — stops capture cleanly via RAII. Receivers
    // still hold buffered frames we already drained above.
    drop(stream);

    println!();
    println!(
        "captured: mic={} frames, system={} frames (lagged: mic={}, system={})",
        mic_frames.len(),
        sys_frames.len(),
        mic_lagged,
        sys_lagged
    );

    // Concatenate each channel's frames in arrival order into a fixed-
    // length 30-second buffer. Truncate the tail if we received more
    // than 30 seconds; zero-pad the tail if we received less. Do NOT
    // place samples by monotonic_ms — see the module docstring for the
    // rationale (millisecond-quantization vs. 20 ms frame size produces
    // sub-millisecond zero-wedge clicks).
    let total_samples = (CAPTURE_SECONDS as usize) * (SAMPLE_RATE_HZ as usize);
    let mic_pcm = frames_to_concat_pcm(&mic_frames, total_samples);
    let sys_pcm = frames_to_concat_pcm(&sys_frames, total_samples);

    let mic_peak = mic_pcm.iter().map(|s| s.unsigned_abs()).max().unwrap_or(0);
    let sys_peak = sys_pcm.iter().map(|s| s.unsigned_abs()).max().unwrap_or(0);
    let mic_silence = max_contiguous_silence(&mic_pcm);
    let sys_silence = max_contiguous_silence(&sys_pcm);

    // Write the WAV. 2-channel interleaved: sample[2*i] = mic, sample[2*i+1] = system.
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: SAMPLE_RATE_HZ,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let output_path = PathBuf::from(OUTPUT_PATH);
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut writer = hound::WavWriter::create(&output_path, spec)?;
    for i in 0..total_samples {
        writer.write_sample(mic_pcm[i])?; // LEFT  = mic
        writer.write_sample(sys_pcm[i])?; // RIGHT = system
    }
    writer.finalize()?;

    let absolute = std::fs::canonicalize(&output_path).unwrap_or(output_path.clone());
    println!();
    println!("=== ear-test summary ===");
    println!("file:               {}", absolute.display());
    println!("duration:           {CAPTURE_SECONDS} s @ {SAMPLE_RATE_HZ} Hz");
    println!("mic    peak (abs):  {mic_peak}");
    println!("system peak (abs):  {sys_peak}");
    println!("mic    longest silence: {}", fmt_silence(mic_silence));
    println!("system longest silence: {}", fmt_silence(sys_silence));
    if mic_peak < 1000 {
        println!("WARNING: mic peak < 1000 — was the mic muted, or no one spoke?");
    }
    if sys_peak < 1000 {
        println!("WARNING: system peak < 1000 — was system audio actually playing?");
    }
    if mic_silence > SILENCE_WINDOW_SAMPLES {
        println!("WARNING: mic has > 1 second of contiguous silence");
    }
    if sys_silence > SILENCE_WINDOW_SAMPLES {
        println!("WARNING: system has > 1 second of contiguous silence");
    }

    println!();
    println!("=== human ear-test checklist (open the WAV with headphones) ===");
    println!("[ ] mic on LEFT channel, system on RIGHT channel (no channel swap)");
    println!("[ ] both channels audible — neither is silent");
    println!("[ ] no silence on either channel during the middle 28 seconds");
    println!("[ ] no clipping (no harsh distortion at peaks)");
    println!("[ ] both channels sound time-aligned — when you spoke, music kept playing underneath without jumps");

    Ok(())
}

/// Concatenate a list of received `Frame`s into a fixed-length PCM
/// buffer by appending each frame's samples in arrival order. If the
/// concatenated content is shorter than `total_samples`, the tail is
/// zero-padded; if longer, the tail is truncated. This is sample-exact
/// within each channel (no millisecond-quantization wedges) at the cost
/// of allowing the two channels to drift by up to one frame (20 ms)
/// relative to each other if one side drops a frame. See the module
/// docstring for the rationale.
fn frames_to_concat_pcm(frames: &[Frame], total_samples: usize) -> Vec<i16> {
    let mut out = Vec::with_capacity(total_samples);
    for frame in frames {
        debug_assert_eq!(frame.samples.len(), FRAME_SAMPLES);
        if out.len() >= total_samples {
            break;
        }
        let room = total_samples - out.len();
        let n = frame.samples.len().min(room);
        out.extend_from_slice(&frame.samples[..n]);
    }
    if out.len() < total_samples {
        out.resize(total_samples, 0);
    }
    out
}

/// Longest contiguous run of below-`SILENCE_THRESHOLD` samples in the
/// buffer. Used for the warn-only summary line — the human is the
/// final judge.
fn max_contiguous_silence(pcm: &[i16]) -> usize {
    let mut longest = 0usize;
    let mut current = 0usize;
    for &s in pcm {
        if s.unsigned_abs() < SILENCE_THRESHOLD as u16 {
            current += 1;
            if current > longest {
                longest = current;
            }
        } else {
            current = 0;
        }
    }
    longest
}

fn fmt_silence(samples: usize) -> String {
    let ms = (samples * 1000) / (SAMPLE_RATE_HZ as usize);
    format!("{samples} samples (~{ms} ms)")
}

#[cfg(test)]
mod tests {
    use super::*;
    use yogurt_audio::Channel;

    /// Regression test for the "static/choppy WAV" bug fixed alongside
    /// this function. The prior `frames_to_aligned_pcm` placed frames
    /// into the WAV buffer at sample-offset `monotonic_ms * 16`. Because
    /// `monotonic_ms` is wall-clock-derived (truncated to whole ms) but
    /// frames are exactly 320 samples = 20 ms each, normal wall-clock
    /// jitter of ±1-2 ms per frame caused consecutive frames to be
    /// placed with 16-sample-aligned gaps (zero-filled) or overlaps
    /// across the entire output buffer. Audible as constant crackle on
    /// the noisier-jitter channel (SCK system audio in our measurements:
    /// 50%+ of post-warmup samples were exact zero, with the zero-run
    /// length histogram peaking at exactly 16 samples).
    ///
    /// `frames_to_concat_pcm` concatenates by arrival order: 50 frames
    /// of 320 non-zero samples each → 16_000 non-zero samples, period.
    /// `monotonic_ms` jitter doesn't matter because we never index by it.
    #[test]
    fn concat_pcm_introduces_no_zero_wedges_under_jitter() {
        // Simulate 50 frames (1 second) of "music" (all samples = 1234)
        // with extremely jittery monotonic_ms values — exactly the case
        // that broke the previous implementation. monotonic_ms here
        // jumps backward, repeats, and gaps wildly to prove the new
        // implementation no longer cares.
        let jittery_ms_values: Vec<u64> = (0..50)
            .map(|i| {
                // Base = i * 20 (perfect 20ms spacing), with chaotic
                // jitter that defeats millisecond-bucketed placement.
                let base = i as u64 * 20;
                match i % 5 {
                    0 => base.saturating_sub(2), // 2 ms early
                    1 => base + 1,               // 1 ms late
                    2 => base.saturating_sub(1), // 1 ms early
                    3 => base + 3,               // 3 ms late
                    _ => base,
                }
            })
            .collect();
        let frames: Vec<Frame> = jittery_ms_values
            .into_iter()
            .map(|ms| Frame::new(Channel::System, ms, vec![1234i16; FRAME_SAMPLES]))
            .collect();

        // Request 1 second of output (= 50 frames worth of samples
        // exactly). All 16_000 samples must equal 1234.
        let total = SAMPLE_RATE_HZ as usize;
        let pcm = frames_to_concat_pcm(&frames, total);
        assert_eq!(pcm.len(), total, "concat must return exactly total_samples");

        // The bug-witnessing assertion: zero non-1234 samples anywhere.
        // Under the old timestamp-bucketed scheme this would fail with
        // many 1ms (16-sample) wedges of zeros scattered throughout.
        let zero_count = pcm.iter().filter(|&&s| s == 0).count();
        assert_eq!(
            zero_count, 0,
            "concat must not introduce any zero-wedges between frames \
             even when monotonic_ms is jittery; found {zero_count} zero samples"
        );
        let wrong_value = pcm.iter().filter(|&&s| s != 1234).count();
        assert_eq!(
            wrong_value, 0,
            "all samples should be the source value 1234; found {wrong_value} mismatches"
        );
    }

    /// When fewer frames arrived than fill `total_samples`, the tail
    /// must be silence-padded (not garbage / not truncated short).
    #[test]
    fn concat_pcm_pads_short_input_with_silence() {
        let frames = vec![Frame::new(Channel::Mic, 0, vec![100i16; FRAME_SAMPLES])];
        let total = FRAME_SAMPLES * 3;
        let pcm = frames_to_concat_pcm(&frames, total);
        assert_eq!(pcm.len(), total);
        assert!(pcm[..FRAME_SAMPLES].iter().all(|&s| s == 100));
        assert!(pcm[FRAME_SAMPLES..].iter().all(|&s| s == 0));
    }

    /// When more frames arrived than fit, truncate the tail (don't
    /// overflow the output buffer).
    #[test]
    fn concat_pcm_truncates_long_input() {
        let frames: Vec<Frame> = (0..5)
            .map(|i| Frame::new(Channel::Mic, i * 20, vec![1i16; FRAME_SAMPLES]))
            .collect();
        let total = FRAME_SAMPLES * 2; // room for only 2 frames
        let pcm = frames_to_concat_pcm(&frames, total);
        assert_eq!(pcm.len(), total);
        assert!(pcm.iter().all(|&s| s == 1));
    }
}
