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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "yogurt=info,yogurt_server=info".into()),
        )
        .init();

    let cli = Cli::parse();
    match cli.command {
        Cmd::Start(args) => {
            commands::start::run(commands::start::StartArgs {
                port: args.port,
                no_open: args.no_open,
                dev: args.dev,
            })
            .await
        }
    }
}
