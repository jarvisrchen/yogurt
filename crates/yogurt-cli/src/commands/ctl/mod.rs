//! `yogurt ctl` (CLI-4 / D1) -- an agent-facing control surface for a
//! running `yogurt` instance, replacing the curl recipes in
//! `.claude/skills/yogurt-control/SKILL.md` and `docs/AI-INTEGRATION.md`
//! with real subcommands: `--json` output, descriptive errors that say
//! what to run instead, idempotent mutations, subcommands over flag soup.
//!
//! See `docs/.planning/agent-workflow.md` section 4D (D1) for the design
//! this implements, and `client.rs` for the discovery precedence
//! (`--port` / `$YOGURT_PORT` / a health scan of 7878-7898).

mod client;
mod detect_cmd;
mod meeting;
mod status;

use clap::{Args, Subcommand};
use serde_json::json;

pub use client::CtlError;

#[derive(Args, Debug)]
pub struct CtlArgs {
    /// Print machine-readable JSON instead of compact text.
    #[arg(long, global = true)]
    pub json: bool,
    /// Talk to the yogurt instance on this port instead of discovering one.
    #[arg(long, global = true)]
    pub port: Option<u16>,
    #[command(subcommand)]
    pub command: Option<CtlCmd>,
}

#[derive(Subcommand, Debug)]
pub enum CtlCmd {
    /// Instances found, active/detected meeting, stt engine, provider, permission grants.
    Status,
    /// Create, start, stop, and read meetings on a running instance.
    Meeting {
        #[command(subcommand)]
        cmd: meeting::MeetingCmd,
    },
    /// What meeting detection currently sees (MTG-11), or dismiss the prompt.
    Detect {
        #[command(subcommand)]
        action: Option<detect_cmd::DetectAction>,
    },
    /// On-screen windows and each one's meeting-detection verdict. No server needed.
    Windows,
}

/// Dispatch + top-level error formatting. Returns the process exit code:
/// `0` on success, `1` on a business-logic error (`error: ... / help:
/// ...` already printed). Usage errors (bad flags, missing args) never
/// reach here -- clap exits `2` on its own during `Cli::parse()`.
pub async fn run(args: CtlArgs) -> i32 {
    let json_out = args.json;
    let port = args.port;
    let result = match args.command.unwrap_or(CtlCmd::Status) {
        CtlCmd::Status => status::run(port, json_out).await,
        CtlCmd::Meeting { cmd } => meeting::run(cmd, port, json_out).await,
        CtlCmd::Detect { action } => detect_cmd::run_detect(port, json_out, action).await,
        CtlCmd::Windows => detect_cmd::run_windows(json_out).await,
    };
    match result {
        Ok(()) => 0,
        Err(e) => print_error(json_out, &e),
    }
}

fn print_error(json_out: bool, err: &CtlError) -> i32 {
    let (message, help) = err.message_and_help();
    if json_out {
        println!("{}", json!({ "error": message, "help": help }));
    } else {
        println!("error: {message}");
        println!("help: {help}");
    }
    1
}
