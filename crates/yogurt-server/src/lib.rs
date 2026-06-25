mod assets;
mod dev_proxy;
mod routes;

use anyhow::Result;
use std::net::SocketAddr;
use tokio::net::TcpListener;

/// Server runtime mode.
///
/// In `Dev`, non-API requests proxy to a Vite dev server on :5173.
/// In `Release`, non-API requests serve embedded `web/dist` assets via
/// `rust-embed` with an SPA fallback to `index.html`.
#[derive(Debug, Clone, Copy)]
pub enum Mode {
    Dev,
    Release,
}

pub async fn run(addr: SocketAddr, mode: Mode) -> Result<()> {
    let app = routes::router(mode);
    tracing::info!(?addr, ?mode, "yogurt-server starting");
    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
