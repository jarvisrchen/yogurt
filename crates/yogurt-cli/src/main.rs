mod commands;

use clap::{Parser, Subcommand};

/// yogurt — local-first meeting copilot.
#[derive(Parser, Debug)]
#[command(name = "yogurt", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Launch the local server and open the browser.
    Start(StartArgs),
    /// Print diagnostic info (rust, macOS, perms, providers, models) + repair actions.
    Doctor(DoctorArgs),
    /// Control a running yogurt instance: status, meetings, detection, windows.
    Ctl(commands::ctl::CtlArgs),
}

#[derive(clap::Args, Debug)]
struct StartArgs {
    /// TCP port to bind.
    #[arg(long, default_value_t = 7878)]
    port: u16,
    /// Do not auto-open the browser on start.
    #[arg(long)]
    no_open: bool,
    /// Run in dev mode (proxies non-API routes to Vite on :5173).
    #[arg(long)]
    dev: bool,
}

#[derive(clap::Args, Debug)]
struct DoctorArgs {
    /// Emit diagnostics as JSON.
    #[arg(long)]
    json: bool,
    /// Reset Screen Recording TCC permission for ai.yogurt.app (forces re-prompt).
    #[arg(long)]
    reset_screen_recording: bool,
    /// Check whether port 7878 is in use.
    #[arg(long)]
    check_port: bool,
    /// Re-download a whisper.cpp model (e.g. small.en) by deleting the local copy.
    #[arg(long, value_name = "MODEL")]
    redownload_model: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // SET-11 (Phase 5 / Plan 05-02): load `.env.local` ONLY in dev mode.
    //
    // The `--dev` check is done via raw `std::env::args` BEFORE `Cli::parse`
    // so the loaded env vars are visible to the parser (clap consults the
    // environment for any `#[arg(env = ...)]` defaults) and to
    // `yogurt-server`'s `bootstrap::seed_from_env` later in the boot chain.
    //
    // Release builds invoked WITHOUT `--dev` MUST NOT touch `.env.local` —
    // brew users never have one, and reading a sibling file would be a
    // surprise. The guard is the raw arg check, not the clap-parsed value,
    // so the file is read at most once regardless of clap configuration.
    //
    // Errors are silently ignored: a missing or unreadable `.env.local` is
    // a normal case (developer hasn't created one yet).
    if std::env::args().any(|a| a == "--dev") {
        let _ = dotenvy::from_filename(".env.local");
    }

    // LO-02: parse args BEFORE installing the tracing subscriber so
    // --help / --version paths never have a chance to emit a startup log
    // line. Any future `tracing::info!` added before `Cli::parse()` would
    // otherwise leak into help output.
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                // EnvFilter matches targets by prefix, so `yogurt=info` already
                // covers `yogurt_server`, `yogurt_audio`, `yogurt_stt`, and every
                // other `yogurt_*` crate - no per-crate directive needed.
                //
                // `whisper_rs=error` covers the C-side logs that
                // `install_logging_hooks()` redirects into `tracing`: ggml and
                // whisper.cpp emit their backend/model banners at INFO on every
                // decode, which is noise. CLI-2 dropped their WARN level too -
                // the one that actually shows up is ggml's
                // `tensor API disabled for pre-M5 and pre-A19 devices`, a
                // hardware-capability note on every load of every model on
                // this machine. A real decode failure still surfaces as an
                // ERROR here, and as an `stt_error` frame in the UI.
                // `RUST_LOG=whisper_rs=debug` brings the whole stream back.
                .unwrap_or_else(|_| "yogurt=info,whisper_rs=error".into()),
        )
        .init();

    match cli.command {
        Cmd::Start(args) => {
            commands::start::run(commands::start::StartArgs {
                port: args.port,
                no_open: args.no_open,
                dev: args.dev,
            })
            .await
        }
        Cmd::Doctor(args) => {
            commands::doctor::run(commands::doctor::DoctorArgs {
                json: args.json,
                reset_screen_recording: args.reset_screen_recording,
                check_port: args.check_port,
                redownload_model: args.redownload_model,
            })
            .await
        }
        Cmd::Ctl(args) => {
            // CLI-4: ctl formats its own `error: ... / help: ...` pair and
            // picks its own exit code (0 success, 1 business-logic error;
            // clap already handles usage errors with exit 2 before this
            // ever runs) -- std::process::exit rather than bubbling an
            // anyhow::Result up through main's generic "Error: {e}" tail.
            let code = commands::ctl::run(args).await;
            std::process::exit(code);
        }
    }
}
