//! Manual smoke test for the full `start_capture()` pipeline (mic + system
//! audio concurrently). Run on a dev Mac with Screen Recording granted,
//! something playing system audio (Spotify, YouTube), and talking into
//! the mic:
//!
//! ```sh
//! cargo run -p yogurt-audio --example dual_smoke
//! ```
//!
//! Expected over 10 seconds: ~500 frames per channel; peak amplitude > 1000
//! on both channels when both audio sources are active.
//!
//! This is NOT the Phase 2 ear-test gate — Plan 02-03 will model that as a
//! `checkpoint:human-verify` task (the human listens to a WAV produced from
//! these frames and confirms both channels are audible, not silent, not
//! clipped, not swapped). This smoke just proves the broadcast plumbing
//! is alive.

use std::time::Duration;
use yogurt_audio::{start_capture, Channel};

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("yogurt_audio=info,dual_smoke=info")
        .init();

    let stream = start_capture()?;
    let mut mic_rx = stream.subscribe_mic();
    let mut sys_rx = stream.subscribe_system();

    tracing::info!("dual capture running; collecting 10 seconds — talk + play music");

    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let mut mic_count = 0u64;
    let mut sys_count = 0u64;
    let mut mic_peak: i16 = 0;
    let mut sys_peak: i16 = 0;

    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        tokio::select! {
            res = mic_rx.recv() => match res {
                Ok(frame) => {
                    debug_assert_eq!(frame.channel, Channel::Mic);
                    mic_count += 1;
                    for &s in &frame.samples {
                        if s.unsigned_abs() > mic_peak.unsigned_abs() {
                            mic_peak = s;
                        }
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(channel = "mic", lagged = n, "consumer fell behind");
                }
                Err(_) => break,
            },
            res = sys_rx.recv() => match res {
                Ok(frame) => {
                    debug_assert_eq!(frame.channel, Channel::System);
                    sys_count += 1;
                    for &s in &frame.samples {
                        if s.unsigned_abs() > sys_peak.unsigned_abs() {
                            sys_peak = s;
                        }
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(channel = "system", lagged = n, "consumer fell behind");
                }
                Err(_) => break,
            },
            _ = tokio::time::sleep(remaining) => break,
        }
    }

    println!("=== mic    channel ===  frames: {mic_count:>4}  peak: {mic_peak:>6}");
    println!("=== system channel ===  frames: {sys_count:>4}  peak: {sys_peak:>6}");
    println!();
    println!("expected over 10s: ~500 frames per channel, peak > 1000 when audio present");
    Ok(())
}
