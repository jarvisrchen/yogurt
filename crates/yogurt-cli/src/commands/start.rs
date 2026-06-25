use anyhow::Result;
use std::net::SocketAddr;
use yogurt_server::Mode;

pub struct StartArgs {
    pub port: u16,
    pub no_open: bool,
    pub dev: bool,
}

pub async fn run(args: StartArgs) -> Result<()> {
    // Localhost-only bind per D-11.
    let addr: SocketAddr = ([127, 0, 0, 1], args.port).into();
    let mode = if args.dev { Mode::Dev } else { Mode::Release };
    let url = format!("http://127.0.0.1:{}", args.port);

    if !args.no_open {
        // Open in a background task so a failure to spawn the browser doesn't
        // block the server from starting.
        let url_for_open = url.clone();
        tokio::spawn(async move {
            if let Err(e) = open::that(&url_for_open) {
                tracing::warn!(?e, "failed to open browser");
            }
        });
    }

    tracing::info!(%url, "yogurt is starting");
    yogurt_server::run(addr, mode).await
}
