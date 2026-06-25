//! Manual smoke test for system audio loopback capture. Run on a dev Mac
//! with Screen Recording permission granted and **audio playing** in another
//! window (Spotify, YouTube, etc.):
//!
//! ```sh
//! cargo run -p yogurt-audio --example system_smoke
//! ```
//!
//! Expected: ~250 frames over 5 seconds, peak amplitude > 1000 when audio
//! is actually playing. Peak ≈ 0 means SCK is delivering buffers but they
//! contain silence — verify audio is playing system-wide and not just in
//! a browser tab that's muted.
//!
//! If you see `PermissionDenied`, grant Screen Recording to the terminal
//! you're invoking `cargo` from in System Settings → Privacy & Security
//! → Screen Recording, then **restart the terminal** (TCC quirk per D-24).

use std::time::Duration;
use tokio::sync::broadcast;
use yogurt_audio::{
    has_screen_recording_permission, spawn_system_capture, Frame, PermissionStatus,
};

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("yogurt_audio=info,system_smoke=info")
        .init();

    match has_screen_recording_permission() {
        PermissionStatus::Granted | PermissionStatus::NotRequired => {}
        PermissionStatus::Denied => {
            eprintln!("Screen Recording permission denied.");
            eprintln!();
            eprintln!("Recovery:");
            eprintln!("  1. Open System Settings → Privacy & Security → Screen Recording");
            eprintln!("  2. Enable for the terminal running this command (Terminal.app, iTerm, etc.)");
            eprintln!("  3. Restart the terminal (TCC requires a process restart)");
            eprintln!("  4. Re-run: cargo run -p yogurt-audio --example system_smoke");
            std::process::exit(2);
        }
    }

    let (tx, mut rx) = broadcast::channel::<Frame>(128);
    let _capture = spawn_system_capture(tx)?;
    tracing::info!("system capture running; collecting 5 seconds — play some audio NOW");

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
    println!("expected: ~250 frames; peak > 1000 when system audio is playing");
    Ok(())
}
