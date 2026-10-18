//! 30-second dual-channel WAV ear-test: captures mic and system audio into
//! `target/yogurt-audio-eartest.wav` (16 kHz, 16-bit, left = mic, right = system).
//!
//! 1. Grant Screen Recording permission to the binary. The `permission` test or
//!    the `system_smoke` example triggers the prompt once.
//! 2. Play audio from Spotify or YouTube in another window for the full 30 seconds.
//! 3. Run `cargo run -p yogurt-audio --example wav_eartest --release` and talk into the mic.
//! 4. Open the WAV with headphones and check the printed list: mic left, system right,
//!    both audible, no silence, no clipping.
//!
//! Frames are concatenated per channel in arrival order, not placed by `monotonic_ms`.
//! `monotonic_ms` is truncated to whole milliseconds while frames are exactly 20 ms,
//! so timestamp placement leaves 16-sample zero gaps that sound like constant crackle.
//! The channels may drift up to one frame relative to each other if one side drops.

use std::path::PathBuf;
use std::time::Duration;

use yogurt_audio::{start_capture, Channel, Frame, FRAME_SAMPLES, SAMPLE_RATE_HZ};

const CAPTURE_SECONDS: u64 = 30;
const OUTPUT_PATH: &str = "target/yogurt-audio-eartest.wav";

const SILENCE_WINDOW_SAMPLES: usize = SAMPLE_RATE_HZ as usize;
/// About -44 dBFS: below a quiet room's mic noise floor, above tiny DC offsets.
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

    drop(stream);

    println!();
    println!(
        "captured: mic={} frames, system={} frames (lagged: mic={}, system={})",
        mic_frames.len(),
        sys_frames.len(),
        mic_lagged,
        sys_lagged
    );

    let total_samples = (CAPTURE_SECONDS as usize) * (SAMPLE_RATE_HZ as usize);
    let mic_pcm = frames_to_concat_pcm(&mic_frames, total_samples);
    let sys_pcm = frames_to_concat_pcm(&sys_frames, total_samples);

    let mic_peak = mic_pcm.iter().map(|s| s.unsigned_abs()).max().unwrap_or(0);
    let sys_peak = sys_pcm.iter().map(|s| s.unsigned_abs()).max().unwrap_or(0);
    let mic_silence = max_contiguous_silence(&mic_pcm);
    let sys_silence = max_contiguous_silence(&sys_pcm);

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

/// Appends frames in arrival order, zero-padding or truncating the tail to `total_samples`.
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

    #[test]
    fn concat_pcm_introduces_no_zero_wedges_under_jitter() {
        let jittery_ms_values: Vec<u64> = (0..50)
            .map(|i| {
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

        let total = SAMPLE_RATE_HZ as usize;
        let pcm = frames_to_concat_pcm(&frames, total);
        assert_eq!(pcm.len(), total, "concat must return exactly total_samples");

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

    #[test]
    fn concat_pcm_pads_short_input_with_silence() {
        let frames = vec![Frame::new(Channel::Mic, 0, vec![100i16; FRAME_SAMPLES])];
        let total = FRAME_SAMPLES * 3;
        let pcm = frames_to_concat_pcm(&frames, total);
        assert_eq!(pcm.len(), total);
        assert!(pcm[..FRAME_SAMPLES].iter().all(|&s| s == 100));
        assert!(pcm[FRAME_SAMPLES..].iter().all(|&s| s == 0));
    }

    #[test]
    fn concat_pcm_truncates_long_input() {
        let frames: Vec<Frame> = (0..5)
            .map(|i| Frame::new(Channel::Mic, i * 20, vec![1i16; FRAME_SAMPLES]))
            .collect();
        let total = FRAME_SAMPLES * 2;
        let pcm = frames_to_concat_pcm(&frames, total);
        assert_eq!(pcm.len(), total);
        assert!(pcm.iter().all(|&s| s == 1));
    }
}
