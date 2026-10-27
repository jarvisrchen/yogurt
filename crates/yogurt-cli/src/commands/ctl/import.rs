//! Granola import: save the key, preview, start (optionally following progress), and cancel.
//! Every command calls the server's `/api/import/granola*` routes. `ctl` never talks to Granola itself.

use clap::{Args, Subcommand};
use futures_util::StreamExt;
use serde_json::{json, Value};
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

use super::client::{Client, CtlError};

const BASE: &str = "/api/import/granola";
/// How long the stream may stay quiet before `--wait` asks the server whether the import already ended.
/// A dropped frame would otherwise hang the command.
const QUIET: Duration = Duration::from_secs(5);

#[derive(Subcommand, Debug)]
pub enum ImportCmd {
    /// Import every note the Granola API key can see. Notes already imported are skipped.
    Granola(GranolaArgs),
}

#[derive(Args, Debug)]
pub struct GranolaArgs {
    /// Count what an import would do without importing anything.
    #[arg(long)]
    dry_run: bool,
    /// Follow progress on stderr until the import ends.
    #[arg(long)]
    wait: bool,
    #[command(subcommand)]
    cmd: Option<GranolaSub>,
}

#[derive(Subcommand, Debug)]
enum GranolaSub {
    /// The Granola API key, stored in keys.json.
    Key {
        #[command(subcommand)]
        cmd: KeyCmd,
    },
    /// Stop a running import. Meetings already imported stay.
    Cancel,
}

#[derive(Subcommand, Debug)]
enum KeyCmd {
    /// Check the key with Granola and save it. Reads the key from stdin so it stays out of shell history.
    Set,
}

pub async fn run(cmd: ImportCmd, port: Option<u16>, json_out: bool) -> Result<(), CtlError> {
    let ImportCmd::Granola(args) = cmd;
    let c = Client::discover(port).await?;
    match args.cmd {
        Some(GranolaSub::Key { cmd: KeyCmd::Set }) => set_key(&c, json_out).await,
        Some(GranolaSub::Cancel) => {
            c.delete_no_body(BASE).await.map_err(with_help)?;
            say(
                json_out,
                json!({ "status": "cancelling" }),
                "cancelling the import",
            );
            Ok(())
        }
        None if args.dry_run => dry_run(&c, json_out).await,
        None => start(&c, json_out, args.wait).await,
    }
}

fn say(json_out: bool, v: Value, text: &str) {
    if json_out {
        println!("{v}");
    } else {
        println!("{text}");
    }
}

/// Points at the one command that fixes a missing key.
fn with_help(e: CtlError) -> CtlError {
    match e {
        CtlError::Server(m) if m.contains("API key") => CtlError::local(
            m,
            "create a key in Granola (Settings, Connectors, API keys), then pipe it into `yogurt ctl import granola key set`",
        ),
        e => e,
    }
}

async fn set_key(c: &Client, json_out: bool) -> Result<(), CtlError> {
    let key = std::io::read_to_string(std::io::stdin())
        .map_err(|e| CtlError::local(format!("could not read stdin: {e}"), "pipe the key in"))?;
    let key = key.trim();
    if key.is_empty() {
        return Err(CtlError::local(
            "no key on stdin",
            "pipe the key in, for example `pbpaste | yogurt ctl import granola key set`",
        ));
    }
    let v: Value = c
        .put(&format!("{BASE}/key"), &json!({ "key": key }))
        .await
        .map_err(with_help)?;
    say(
        json_out,
        json!({ "status": "saved", "key": v["key"] }),
        &format!("saved Granola key {}", v["key"].as_str().unwrap_or("")),
    );
    Ok(())
}

async fn dry_run(c: &Client, json_out: bool) -> Result<(), CtlError> {
    let v: Value = c
        .post_empty(&format!("{BASE}/preview"))
        .await
        .map_err(with_help)?;
    if json_out {
        println!("{}", json!({ "dry_run": true, "preview": v }));
    } else {
        println!(
            "{} notes in Granola, {} already imported, {} to import, {} captured by someone else",
            v["total"], v["already_imported"], v["to_import"], v["captured_by_others"]
        );
    }
    Ok(())
}

async fn start(c: &Client, json_out: bool, wait: bool) -> Result<(), CtlError> {
    // Connect first so the first frames are not missed.
    let stream = if wait {
        Some(super::ws::connect(c, "/ws").await?)
    } else {
        None
    };
    c.post_no_body(BASE).await.map_err(with_help)?;
    say(json_out, json!({ "status": "started" }), "import started");
    match stream {
        Some(s) => follow(c, s, json_out).await,
        None => Ok(()),
    }
}

async fn follow(
    c: &Client,
    mut stream: super::ws::WsStream,
    json_out: bool,
) -> Result<(), CtlError> {
    loop {
        match tokio::time::timeout(QUIET, stream.next()).await {
            Ok(Some(Ok(Message::Text(t)))) => {
                let Ok(v) = serde_json::from_str::<Value>(&t) else {
                    continue;
                };
                match v["type"].as_str() {
                    Some("import_progress") => eprintln!(
                        "progress: {} of {} (imported {}, skipped {}, failed {})",
                        v["done"], v["total"], v["imported"], v["skipped"], v["failed"]
                    ),
                    Some("import_complete") => return finish(&v, json_out),
                    Some("import_cancelled") => {
                        return ended(&v, "the import was cancelled".into())
                    }
                    Some("import_error") => return ended(&v, failed(&v)),
                    _ => {}
                }
            }
            Ok(Some(Ok(_))) => {}
            Ok(Some(Err(e))) => {
                return Err(CtlError::local(
                    format!("websocket error: {e}"),
                    "check `yogurt ctl import granola --dry-run` and try again",
                ))
            }
            Ok(None) => {
                return Err(CtlError::local(
                    "server closed the connection while the import was running",
                    "the import may still be running, so check `yogurt ctl status`",
                ))
            }
            Err(_) => {
                let st: Value = c.get(BASE).await?;
                if st["running"] == json!(false) {
                    let last = &st["last"];
                    return match last["outcome"].as_str() {
                        Some("complete") => finish(last, json_out),
                        Some("cancelled") => ended(last, "the import was cancelled".into()),
                        _ => ended(last, failed(last)),
                    };
                }
            }
        }
    }
}

fn finish(v: &Value, json_out: bool) -> Result<(), CtlError> {
    if json_out {
        println!("{v}");
        return Ok(());
    }
    println!(
        "imported {}, skipped {}, failed {}",
        v["imported"], v["skipped"], v["failed"]
    );
    for t in v["failed_titles"].as_array().into_iter().flatten() {
        println!("failed: {}", t.as_str().unwrap_or("?"));
    }
    Ok(())
}

fn failed(v: &Value) -> String {
    format!(
        "the import failed: {}",
        v["error"].as_str().unwrap_or("unknown error")
    )
}

fn ended(v: &Value, what: String) -> Result<(), CtlError> {
    Err(CtlError::local(
        format!(
            "{what}{}",
            if v["imported"].is_u64() {
                format!(" after importing {}", v["imported"])
            } else {
                String::new()
            }
        ),
        "run `yogurt ctl import granola --wait` to resume where it stopped",
    ))
}
