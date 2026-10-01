//! `yogurt ctl detect [dismiss]` and `yogurt ctl windows` (CLI-4).
//!
//! `detect` reflects the *server's* MTG-11 polling loop (`GET`/`POST
//! /api/meetings/detected*`), so it needs a running instance. `windows` is
//! the promoted `meeting_windows` cargo example: in-process `SCShareableContent`
//! enumeration via `yogurt_audio::detect::scan_windows`, no server involved.
//! It also lists the processes holding the microphone with their meeting-app
//! verdict (`yogurt_audio::mic_usage`), the signal auto-stop watches.

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

    let own_pid = std::process::id() as i32;
    let mic = tokio::task::spawn_blocking(yogurt_audio::mic_usage::mic_users)
        .await
        .map_err(|e| {
            CtlError::local(
                format!("microphone scan panicked: {e}"),
                "retry `yogurt ctl windows`",
            )
        })?;

    if json_out {
        let mic_json = mic.as_ref().map(|users| {
            users
                .iter()
                .map(|u| {
                    json!({
                        "pid": u.pid,
                        "bundle": u.bundle_id,
                        "meeting_app": yogurt_audio::detect::meeting_app_for_process(&u.bundle_id)
                            .filter(|_| u.pid != own_pid),
                    })
                })
                .collect::<Vec<_>>()
        });
        println!(
            "{}",
            json!({ "windows": rows, "microphone_users": mic_json })
        );
        return Ok(());
    }

    if rows.is_empty() {
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
    println!();
    match mic {
        None => println!("microphone usage: unsupported on this macOS"),
        Some(users) if users.is_empty() => {
            println!("microphone usage: no process is using the microphone")
        }
        Some(users) => {
            println!("microphone usage:");
            for u in &users {
                let verdict = if u.pid == own_pid {
                    "-"
                } else {
                    yogurt_audio::detect::meeting_app_for_process(&u.bundle_id).unwrap_or("-")
                };
                let bundle = if u.bundle_id.is_empty() {
                    "?"
                } else {
                    &u.bundle_id
                };
                println!("{:<16} {:<40} pid {}", verdict, bundle, u.pid);
            }
        }
    }
    Ok(())
}
