mod api;
mod assets;
pub mod audio;
mod cloud_push;
mod detect;
mod dev_proxy;
pub(crate) mod enhance;
mod granola_import;
pub(crate) mod llm_mock;
pub(crate) mod llm_openai;
pub(crate) mod markdown_exporter;
pub mod meetings;
mod routes;
pub(crate) mod sanitize;
pub mod session;
pub mod state;
pub mod storage;
pub mod ws;

pub mod bootstrap;

#[doc(hidden)]
pub mod __test_only_sanitize {
    pub use crate::sanitize::sanitize_enriched_md;
}

/// `POST /api/meetings/:id/start` opens real ScreenCaptureKit capture, which cannot succeed
/// without Screen Recording permission, so tests call this pure patch function directly.
#[doc(hidden)]
pub mod __test_only_start_stamp {
    pub use crate::routes::start_stamp_patch;
}

#[doc(hidden)]
pub mod __test_only_llm_mock {
    pub use crate::llm_mock::MockLlm;
}

/// Compiled only for tests or the `test-support` feature.
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

#[doc(hidden)]
pub mod __test_only_markdown_exporter {
    pub use crate::markdown_exporter::{MarkdownExporter, Meeting};
}

#[doc(hidden)]
pub fn __test_only_aux_state(
    notes_dir: std::path::PathBuf,
) -> anyhow::Result<(
    std::sync::Arc<markdown_exporter::MarkdownExporter>,
    std::sync::Arc<yogurt_prompts::Prompts>,
)> {
    let me = std::sync::Arc::new(markdown_exporter::MarkdownExporter::new(notes_dir)?);
    let prompts = std::sync::Arc::new(yogurt_prompts::Prompts::load(yogurt_prompts::Mode::Dev)?);
    Ok((me, prompts))
}

use anyhow::Result;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::TcpListener;

pub use session::SessionToken;
pub use state::AppState;
pub use storage::Storage;

/// `Dev` proxies non-API requests to Vite on :5173. `Release` serves the embedded `web/dist`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Dev,
    Release,
}

/// Path overrides let tests and `$YOGURT_DATA_DIR` avoid clobbering the real `~/.yogurt/`.
pub struct RunConfig {
    pub addr: SocketAddr,
    pub mode: Mode,
    pub db_path: Option<PathBuf>,
    pub session_token_path: Option<PathBuf>,
    pub notes_dir: Option<PathBuf>,
    /// Separate from `db_path`: the `yogurt-db` file (providers, settings). Tests pass a
    /// tempdir so parallel suites do not collide on the WAL lock.
    pub app_db_path: Option<PathBuf>,
    pub keys_path: Option<PathBuf>,
}

impl RunConfig {
    pub fn new(addr: SocketAddr, mode: Mode) -> Self {
        Self {
            addr,
            mode,
            db_path: None,
            session_token_path: None,
            notes_dir: None,
            app_db_path: None,
            keys_path: None,
        }
    }
}

pub async fn run(addr: SocketAddr, mode: Mode) -> Result<()> {
    run_with_config(RunConfig::new(addr, mode)).await
}

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

    let notes_dir = match cfg.notes_dir {
        Some(p) => p,
        None => default_notes_dir()?,
    };

    let state = AppState::production(state::ProductionConfig {
        mode: cfg.mode,
        bind_port: cfg.addr.port(),
        storage,
        session,
        notes_dir,
        app_db_path: cfg.app_db_path,
        keys_path: cfg.keys_path,
    })?;

    // .env.local is loaded only in dev mode, by yogurt-cli.
    match bootstrap::seed_from_env(&state).await {
        Ok(report) => {
            if !report.seeded.is_empty() {
                tracing::info!(seeded = ?report.seeded, "seeded providers from env vars");
            }
            if !report.skipped.is_empty() {
                tracing::debug!(skipped = ?report.skipped, "skipped already-configured providers");
            }
        }
        Err(e) => {
            tracing::error!(error = %e, "bootstrap failed; continuing without env-var seeding")
        }
    }

    bootstrap::seed_stt_key_from_env(&state);
    bootstrap::seed_granola_key_from_env(&state);

    if let Err(e) = bootstrap::repair_escaped_span_rows(&state) {
        tracing::warn!(error = %e, "escaped-span repair failed; continuing");
    }

    // Reads its enable flag every tick, so toggling takes effect immediately.
    detect::spawn(state.clone());

    let app = routes::router(state.clone());
    tracing::info!(addr = ?cfg.addr, mode = ?cfg.mode, "yogurt-server starting");
    let listener = TcpListener::bind(cfg.addr).await?;
    api::cloud::spawn_auto_push(state.clone());
    axum::serve(listener, app).await?;
    Ok(())
}

fn default_notes_dir() -> Result<PathBuf> {
    use directories::BaseDirs;
    let base =
        BaseDirs::new().ok_or_else(|| anyhow::anyhow!("could not resolve home directory"))?;
    Ok(base.home_dir().join(".yogurt").join("notes"))
}

#[doc(hidden)]
pub fn __test_router(state: AppState) -> axum::Router {
    routes::router(state)
}
