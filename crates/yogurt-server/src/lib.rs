mod assets;
pub mod audio;
mod dev_proxy;
pub mod meetings;
mod routes;
pub mod session;
pub mod storage;
pub mod ws;

use anyhow::Result;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::TcpListener;

pub use session::SessionToken;
pub use storage::Storage;

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

/// Shared per-request app state.
#[derive(Clone)]
pub struct AppState {
    pub mode: Mode,
    pub storage: Arc<Storage>,
    pub session: Arc<SessionToken>,
    pub bind_port: u16,
    /// Phase 3: in-memory meeting registry. Phase 7 will swap the internal
    /// `HashMap` for SQLite-backed persistence behind the same API.
    pub meetings: Arc<meetings::Registry>,
}

/// Configuration for `run` that can be overridden in tests (custom DB +
/// session-token paths so tests do not clobber the developer's real
/// `~/.yogurt/`).
pub struct RunConfig {
    pub addr: SocketAddr,
    pub mode: Mode,
    pub db_path: Option<PathBuf>,
    pub session_token_path: Option<PathBuf>,
}

impl RunConfig {
    pub fn new(addr: SocketAddr, mode: Mode) -> Self {
        Self {
            addr,
            mode,
            db_path: None,
            session_token_path: None,
        }
    }
}

/// Default entry point. Uses real `~/.yogurt/` paths.
pub async fn run(addr: SocketAddr, mode: Mode) -> Result<()> {
    run_with_config(RunConfig::new(addr, mode)).await
}

/// Configurable entry point — accepts overrides for DB path and session
/// token path (used by tests).
pub async fn run_with_config(cfg: RunConfig) -> Result<()> {
    let db_path = match cfg.db_path {
        Some(p) => p,
        None => storage::default_db_path()?,
    };
    let storage = Arc::new(Storage::init_at(&db_path)?);

    let token_path = match cfg.session_token_path {
        Some(p) => p,
        None => session::default_token_path()?,
    };
    let session = Arc::new(session::load_or_create(&token_path)?);

    let state = AppState {
        mode: cfg.mode,
        storage,
        session,
        bind_port: cfg.addr.port(),
        meetings: meetings::Registry::new(),
    };

    let app = routes::router(state);
    tracing::info!(addr = ?cfg.addr, mode = ?cfg.mode, "yogurt-server starting");
    let listener = TcpListener::bind(cfg.addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

/// Construct a router with a custom AppState, for tests that want to reach
/// into the registry directly instead of going through the REST surface.
///
/// Not part of the stable public API — leading underscore communicates that.
#[doc(hidden)]
pub fn __test_router(state: AppState) -> axum::Router {
    routes::router(state)
}
