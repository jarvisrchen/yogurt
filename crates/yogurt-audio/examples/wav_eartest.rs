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
//! common origin to within microseconds. We bucket samples by their
//! frame's `monotonic_ms` and zero-pad any gaps so both channels stay
//! time-aligned (when one stream drops a frame, the other's
//! corresponding interleaved samples come out against silence on the
//! lagging side — preferable to drifting the channels relative to each
//! other, which would mask channel-swap detection by ear).

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

    let stream = start_capture()?;
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

    // Build a fixed-duration sample buffer per channel, indexed by
    // monotonic_ms. The earliest received frame on either side defines
    // t=0; we render 30 seconds of output samples = 30 * 16_000 = 480_000
    // samples per channel.
    let total_samples = (CAPTURE_SECONDS as usize) * (SAMPLE_RATE_HZ as usize);
    let baseline_ms = mic_frames
        .first()
        .map(|f| f.monotonic_ms)
        .into_iter()
        .chain(sys_frames.first().map(|f| f.monotonic_ms))
        .min()
        .unwrap_or(0);

    let mic_pcm = frames_to_aligned_pcm(&mic_frames, baseline_ms, total_samples);
    let sys_pcm = frames_to_aligned_pcm(&sys_frames, baseline_ms, total_samples);

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

/// Render a list of received `Frame`s into a fixed-length PCM buffer
/// indexed by `monotonic_ms`. Each frame is placed at the sample offset
/// corresponding to its `monotonic_ms - baseline_ms` offset; gaps are
/// zero-padded (keeping the two channels time-aligned for accurate
/// channel-swap detection by ear).
fn frames_to_aligned_pcm(frames: &[Frame], baseline_ms: u64, total_samples: usize) -> Vec<i16> {
    let mut out = vec![0i16; total_samples];
    for frame in frames {
        let rel_ms = frame.monotonic_ms.saturating_sub(baseline_ms);
        // 16 kHz → 16 samples per millisecond. Use saturating arithmetic
        // to handle late-arriving frames that overflow the 30-second
        // window: we just drop their tail.
        let start = (rel_ms as usize).saturating_mul(SAMPLE_RATE_HZ as usize / 1000);
        if start >= total_samples {
            continue;
        }
        debug_assert_eq!(frame.samples.len(), FRAME_SAMPLES);
        let end = (start + frame.samples.len()).min(total_samples);
        let n = end - start;
        out[start..end].copy_from_slice(&frame.samples[..n]);
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
