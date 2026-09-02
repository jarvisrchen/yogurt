//! `yogurt ctl models` (CLI-6 / D1 second slice) -- whisper.cpp STT model
//! management: list, download (optionally following progress on the
//! app-wide `/ws` socket), delete.

use clap::Subcommand;
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::json;
use tokio_tungstenite::tungstenite::Message;

use super::client::{Client, CtlError};

#[derive(Subcommand, Debug)]
pub enum ModelsCmd {
    /// List whisper.cpp models: size, downloaded state.
    List,
    /// Start downloading a model.
    Download {
        name: String,
        /// Follow progress on stderr (via `/ws`) until it completes or fails.
        #[arg(long)]
        wait: bool,
    },
    /// Delete a downloaded model.
    Delete {
        name: String,
        /// Print what would be removed, without removing it.
        #[arg(long)]
        dry_run: bool,
    },
}

pub async fn run(cmd: ModelsCmd, port: Option<u16>, json_out: bool) -> Result<(), CtlError> {
    match cmd {
        ModelsCmd::List => list(port, json_out).await,
        ModelsCmd::Download { name, wait } => download(port, json_out, &name, wait).await,
        ModelsCmd::Delete { name, dry_run } => delete(port, json_out, &name, dry_run).await,
    }
}

/// Mirrors `crate::api::stt_models::ModelView` on the server (skips
/// `intel_supported`, which no `ctl` output needs -- serde ignores the
/// extra JSON field on deserialize).
#[derive(Debug, Deserialize, serde::Serialize)]
struct ModelRow {
    name: String,
    size_mb: u32,
    downloaded: bool,
    managed_by_homebrew: bool,
}

async fn list(port: Option<u16>, json_out: bool) -> Result<(), CtlError> {
    let c = Client::discover(port).await?;
    let models: Vec<ModelRow> = c.get("/api/stt/models").await?;
    if json_out {
        println!("{}", json!({ "models": models }));
    } else {
        println!("{} model(s)", models.len());
        for m in &models {
            let state = if m.downloaded {
                "downloaded"
            } else {
                "not downloaded"
            };
            let src = if m.managed_by_homebrew { " (brew)" } else { "" };
            println!("{:<16} {:>5} MB  {state}{src}", m.name, m.size_mb);
        }
    }
    Ok(())
}

fn find<'a>(models: &'a [ModelRow], name: &str) -> Result<&'a ModelRow, CtlError> {
    models.iter().find(|m| m.name == name).ok_or_else(|| {
        CtlError::local(
            format!("no such model: {name}"),
            "run `yogurt ctl models list`",
        )
    })
}

async fn download(
    port: Option<u16>,
    json_out: bool,
    name: &str,
    wait: bool,
) -> Result<(), CtlError> {
    let c = Client::discover(port).await?;
    // A clear "no such model" beats the server's generic 404 for a typo'd
    // name, and doubles as the existence check `--wait`'s frame filter
    // (matched on `model`) implicitly relies on.
    let models: Vec<ModelRow> = c.get("/api/stt/models").await?;
    find(&models, name)?;
    c.post_no_body(&format!("/api/stt/models/{name}/download"))
        .await?;
    if json_out {
        println!("{}", json!({ "status": "started", "model": name }));
    } else {
        println!("download started: {name}");
    }
    if wait {
        wait_for_download(&c, name, json_out).await?;
    }
    Ok(())
}

/// Follows the model's `stt_model_download_*` frames on the app-wide
/// `/ws` socket (`crates/yogurt-server/src/ws.rs`'s `WsEvent`), printing
/// progress to stderr per the ticket's spec, until `..._complete` /
/// `..._error` or an internal safety timeout.
async fn wait_for_download(c: &Client, name: &str, json_out: bool) -> Result<(), CtlError> {
    let mut stream = super::ws::connect(c, "/ws").await?;
    // ponytail: a flat cap rather than a `--timeout` flag nobody asked
    // for. Model downloads are tens of MB to a few GB; two minutes with
    // no progress at all means something is actually stuck.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(120);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return Err(timed_out(name));
        }
        let msg = match tokio::time::timeout(remaining, stream.next()).await {
            Ok(Some(Ok(Message::Text(t)))) => t,
            Ok(Some(Ok(_))) => continue,
            Ok(Some(Err(e))) => {
                return Err(CtlError::local(
                    format!("websocket error: {e}"),
                    "check `yogurt ctl status`",
                ))
            }
            Ok(None) => {
                return Err(CtlError::local(
                    "server closed the connection while waiting for the download",
                    "check `yogurt ctl status`",
                ))
            }
            Err(_) => return Err(timed_out(name)),
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&msg) else {
            continue;
        };
        if v.get("model").and_then(|m| m.as_str()) != Some(name) {
            continue;
        }
        match v.get("type").and_then(|t| t.as_str()) {
            Some("stt_model_download_progress") => {
                let downloaded = v
                    .get("bytes_downloaded")
                    .and_then(|x| x.as_u64())
                    .unwrap_or(0);
                let total = v.get("total_bytes").and_then(|x| x.as_u64()).unwrap_or(0);
                eprintln!("progress: {downloaded}/{total} bytes");
            }
            Some("stt_model_download_complete") => {
                if json_out {
                    println!("{}", json!({ "status": "complete", "model": name }));
                } else {
                    println!("downloaded {name}");
                }
                return Ok(());
            }
            Some("stt_model_download_error") => {
                let err = v
                    .get("error")
                    .and_then(|e| e.as_str())
                    .unwrap_or("download failed");
                return Err(CtlError::local(
                    format!("download failed: {err}"),
                    "check network connectivity and retry `yogurt ctl models download`",
                ));
            }
            _ => {}
        }
    }
}

fn timed_out(name: &str) -> CtlError {
    CtlError::local(
        format!("timed out waiting for {name} to finish downloading"),
        "run `yogurt ctl models list` to check progress",
    )
}

#[derive(Debug, Deserialize)]
struct DeleteResult {
    freed_bytes: u64,
}

async fn delete(
    port: Option<u16>,
    json_out: bool,
    name: &str,
    dry_run: bool,
) -> Result<(), CtlError> {
    let c = Client::discover(port).await?;
    let models: Vec<ModelRow> = c.get("/api/stt/models").await?;
    let row = find(&models, name)?;

    if dry_run {
        if json_out {
            println!(
                "{}",
                json!({ "dry_run": true, "model": name, "downloaded": row.downloaded })
            );
        } else if row.downloaded {
            println!("would delete {name} (~{} MB)", row.size_mb);
        } else {
            println!("would delete {name} (not downloaded, nothing to remove)");
        }
        return Ok(());
    }

    let result: DeleteResult = c.delete(&format!("/api/stt/models/{name}")).await?;
    if json_out {
        println!(
            "{}",
            json!({ "status": "deleted", "model": name, "freed_bytes": result.freed_bytes })
        );
    } else {
        println!("deleted {name}, freed {} bytes", result.freed_bytes);
    }
    Ok(())
}
