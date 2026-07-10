//! Manual smoke test for mic capture. Run on a dev Mac with a real input
//! device available:
//!
//! ```sh
//! cargo run -p yogurt-audio --example mic_smoke
//! ```
//!
//! Expected: ~250 frames over 5 seconds (= 50 Hz × 5 s, 20 ms cadence) and
//! a peak amplitude above ~1000 when you're speaking near the microphone.
//! If you see ~250 frames but peak ~0, the device is enumerating but no PCM
//! is making it through — check macOS Sound Settings → Input level meter.
//!
//! Not run by `cargo test`. This exists for human verification of the
//! cpal → Downmix → broadcast path.

use std::time::Duration;
use tokio::sync::broadcast;
use yogurt_audio::{spawn_mic_capture, Frame};

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("yogurt_audio=info,mic_smoke=info")
        .init();

    let (tx, mut rx) = broadcast::channel::<Frame>(128);
    let _capture = spawn_mic_capture(tx, None)?;
    tracing::info!("mic capture running; collecting 5 seconds…");

    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    let mut count = 0u64;
    let mut peak: i16 = 0;

    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        match tokio::time::timeout(remaining, rx.recv()).await {
            Ok(Ok(frame)) => {
                count += 1;
                for &s in &frame.samples {
                    if s.unsigned_abs() > peak.unsigned_abs() {
                        peak = s;
                    }
                }
            }
            Ok(Err(broadcast::error::RecvError::Lagged(n))) => {
                tracing::warn!(lagged = n, "consumer fell behind");
            }
            Ok(Err(broadcast::error::RecvError::Closed)) => break,
            Err(_) => break, // deadline elapsed
        }
    }

    println!("frames received: {count}");
    println!("peak amplitude:  {peak}");
    println!();
    println!("expected: ~250 frames; peak > 1000 when speaking, < 200 in silence");
    Ok(())
}
