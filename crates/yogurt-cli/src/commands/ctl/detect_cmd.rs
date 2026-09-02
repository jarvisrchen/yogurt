//! `yogurt ctl detect [dismiss]` and `yogurt ctl windows` (CLI-4).
//!
//! `detect` reflects the *server's* MTG-11 polling loop (`GET`/`POST
//! /api/meetings/detected*`), so it needs a running instance. `windows` is
//! the promoted `meeting_windows` cargo example: in-process `SCShareableContent`
//! enumeration via `yogurt_audio::detect::scan_windows`, no server involved.

use serde_json::json;

use super::client::{self, CtlError};

#[derive(clap::Subcommand, Debug)]
pub enum DetectAction {
    /// Suppress the current detection prompt until a different window matches.
    Dismiss,
}

pub async fn run_detect(
    port_flag: Option<u16>,
    json_out: bool,
    action: Option<DetectAction>,
) -> Result<(), CtlError> {
    let c = client::Client::discover(port_flag).await?;
    match action {
        Some(DetectAction::Dismiss) => {
            let _: serde_json::Value = c.post_empty("/api/meetings/detected/dismiss").await?;
            if json_out {
                println!("{}", json!({ "status": "dismissed" }));
            } else {
                println!("dismissed");
            }
        }
        None => {
            let detected: Option<yogurt_audio::detect::DetectedMeeting> =
                c.get("/api/meetings/detected").await?;
            if json_out {
                println!("{}", json!({ "detected": detected }));
            } else {
                match detected {
                    Some(m) => println!("{} - {}", m.app, m.title),
                    None => println!("nothing detected"),
                }
            }
        }
    }
    Ok(())
}

pub async fn run_windows(json_out: bool) -> Result<(), CtlError> {
    if !cfg!(target_os = "macos") {
        return Err(CtlError::local(
            "windows scan is macOS-only",
            "run this on macOS",
        ));
    }
    // MTG-11: a Denied grant makes every SCK window title come back
    // redacted, so `scan_windows` would silently return an empty list --
    // indistinguishable from "no meeting-looking windows exist". Bail with
    // the exact reason instead, matching `detect_meeting`'s own guard.
    use yogurt_audio::permission::{has_screen_recording_permission, PermissionStatus};
    if has_screen_recording_permission() == PermissionStatus::Denied {
        return Err(CtlError::local(
            "screen recording: denied",
            "grant Screen Recording access in System Settings > Privacy & Security, then retry",
        ));
    }

    let rows = tokio::task::spawn_blocking(yogurt_audio::detect::scan_windows)
        .await
        .map_err(|e| {
            CtlError::local(
                format!("window scan panicked: {e}"),
                "retry `yogurt ctl windows`",
            )
        })?;

    if json_out {
        println!(
            "{}",
            serde_json::to_string(&rows).map_err(|e| CtlError::local(
                format!("could not serialize windows: {e}"),
                "retry `yogurt ctl windows`"
            ))?
        );
    } else if rows.is_empty() {
        println!("no on-screen windows found");
    } else {
        for r in &rows {
            println!(
                "{:<16} {:<40} {}",
                r.verdict.unwrap_or("-"),
                r.bundle,
                r.title
            );
        }
    }
    Ok(())
}
