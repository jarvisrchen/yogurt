//! `yogurt ctl`: an agent-facing control surface for a running instance.
//! `--json` output, errors that say what to run instead, idempotent mutations.

mod client;
mod detect_cmd;
mod meeting;
mod models;
mod provider;
mod settings;
mod status;
mod ws;

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
    /// Instances found, active/detected meeting, stt engine, provider, each server's permission grants.
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
    /// General settings the server exposes: get and set.
    Settings {
        #[command(subcommand)]
        cmd: settings::SettingsCmd,
    },
    /// Configured LLM providers: list, activate, test.
    Provider {
        #[command(subcommand)]
        cmd: provider::ProviderCmd,
    },
    /// STT models: list, download, delete.
    Models {
        #[command(subcommand)]
        cmd: models::ModelsCmd,
    },
    /// Subscribe to the server websocket, printing one JSON frame per line.
    Ws {
        /// Meeting id, url, or `last` -- scopes to that meeting's transcript
        /// socket instead of the app-wide one.
        meeting: Option<String>,
        /// Only print frames whose "type" is one of these (comma-separated).
        #[arg(long, value_delimiter = ',')]
        types: Vec<String>,
        /// Stop after this many matching frames.
        #[arg(long)]
        count: Option<usize>,
    },
}

/// Dispatch and top-level error formatting. Returns `0` on success and `1` on a business-logic error already printed.
/// Clap exits `2` on usage errors before reaching here.
pub async fn run(args: CtlArgs) -> i32 {
    let json_out = args.json;
    let port = args.port;
    let result = match args.command.unwrap_or(CtlCmd::Status) {
        CtlCmd::Status => status::run(port, json_out).await,
        CtlCmd::Meeting { cmd } => meeting::run(cmd, port, json_out).await,
        CtlCmd::Detect { action } => detect_cmd::run_detect(port, json_out, action).await,
        CtlCmd::Windows => detect_cmd::run_windows(json_out).await,
        CtlCmd::Settings { cmd } => settings::run(cmd, port, json_out).await,
        CtlCmd::Provider { cmd } => provider::run(cmd, port, json_out).await,
        CtlCmd::Models { cmd } => models::run(cmd, port, json_out).await,
        CtlCmd::Ws {
            meeting,
            types,
            count,
        } => ws::run(meeting, types, count, port, json_out).await,
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
