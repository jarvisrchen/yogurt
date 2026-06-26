mod assets;
pub mod audio;
mod dev_proxy;
// Phase 4 (Plan 04-03) augmented-notes enhance endpoint handler. Wires
// prompts → LLM → merge → persistence → WS progress events end-to-end.
pub(crate) mod enhance;
// Phase 4 (Plan 04-01) tactical LLM client + mock fallback. Crate-private
// modules — Plan 04-03's enhance handler consumes them directly. NO
// `LlmClient` trait yet — Phase 5 introduces it (CONTEXT D-19 / D-21).
pub(crate) mod llm_mock;
pub(crate) mod llm_openai;
pub(crate) mod markdown_exporter;
pub mod meetings;
mod routes;
// Phase 4 BL-2 (XSS hardening): HTML allowlist sanitizer applied to the
// final enriched_md before it leaves the enhance handler. Re-exported as
// `__test_only_sanitize` for test access.
pub(crate) mod sanitize;
pub mod session;
// Phase 5 (Plan 05-02): `AppState` lives here so it can carry the new
// `yogurt-db::Db` + `Arc<dyn ApiKeyStore>` fields alongside the Phase 4
// fields. Re-exported below as `pub use state::AppState`.
pub mod state;
pub mod storage;
pub mod ws;

// Phase 5 (Plan 05-02): bootstrap providers from `YOGURT_*_API_KEY` env
// vars on first run. Idempotent; release builds never read `.env.local`
// (the loader is gated on `--dev` in `yogurt-cli/src/main.rs`).
pub mod bootstrap;

#[doc(hidden)]
pub mod __test_only_sanitize {
    pub use crate::sanitize::sanitize_enriched_md;
}

/// Phase 4 (Plan 04-02) test-only re-export of the otherwise crate-private
/// `markdown_exporter` module. Wiring (call from the enhance handler) lands
/// in Plan 04-03; until then the module surface is exercised only by
/// `tests/markdown_exporter.rs`. `#[doc(hidden)]` keeps it out of the
/// public docs.
#[doc(hidden)]
pub mod __test_only_markdown_exporter {
    pub use crate::markdown_exporter::{MarkdownExporter, Meeting};
}

/// Phase 4 (Plan 04-03) test-only helper to build the auxiliary `AppState`
/// fields that the integration tests (`tests/meeting_ws*.rs`,
/// `tests/e2e_synthetic_audio.rs`) need. Hidden from docs — production
/// code constructs them inline inside `run_with_config`.
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
// Phase 5 (Plan 05-02): canonical `AppState` lives in `state.rs`. Re-exported
// here so existing call sites (`use crate::AppState`) keep compiling.
pub use state::AppState;
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

/// Configuration for `run` that can be overridden in tests (custom DB +
/// session-token paths so tests do not clobber the developer's real
/// `~/.yogurt/`).
pub struct RunConfig {
    pub addr: SocketAddr,
    pub mode: Mode,
    pub db_path: Option<PathBuf>,
    pub session_token_path: Option<PathBuf>,
    /// Phase 4 (Plan 04-03): override the per-meeting markdown notes
    /// directory. Defaults to `~/.yogurt/notes/`. Tests pass a tempdir.
    pub notes_dir: Option<PathBuf>,
}

impl RunConfig {
    pub fn new(addr: SocketAddr, mode: Mode) -> Self {
        Self {
            addr,
            mode,
            db_path: None,
            session_token_path: None,
            notes_dir: None,
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

    // Phase 4 (Plan 04-03): per-meeting markdown exporter (STORE-03/04).
    let notes_dir = match cfg.notes_dir {
        Some(p) => p,
        None => default_notes_dir()?,
    };

    // Phase 5 (Plan 05-02): construct AppState via the new
    // `state::AppState::production_warmed()` constructor. The `_warmed`
    // variant eager-loads Keychain entries with a 5s timeout (SET-10) so
    // request handlers never block on `keyring` during a request.
    let state = AppState::production_warmed(state::ProductionConfig {
        mode: cfg.mode,
        bind_port: cfg.addr.port(),
        storage,
        session,
        notes_dir,
    })
    .await?;

    // Phase 5 (Plan 05-02): seed providers from `YOGURT_*_API_KEY` env vars
    // (loaded from .env.local in dev mode by yogurt-cli). Idempotent —
    // existing providers are skipped, never overridden.
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

    let app = routes::router(state);
    tracing::info!(addr = ?cfg.addr, mode = ?cfg.mode, "yogurt-server starting");
    let listener = TcpListener::bind(cfg.addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

/// Default per-meeting markdown notes directory: `~/.yogurt/notes/`.
/// Tests override via `RunConfig::notes_dir`.
fn default_notes_dir() -> Result<PathBuf> {
    use directories::BaseDirs;
    let base =
        BaseDirs::new().ok_or_else(|| anyhow::anyhow!("could not resolve home directory"))?;
    Ok(base.home_dir().join(".yogurt").join("notes"))
}

/// Construct a router with a custom AppState, for tests that want to reach
/// into the registry directly instead of going through the REST surface.
///
/// Not part of the stable public API — leading underscore communicates that.
#[doc(hidden)]
pub fn __test_router(state: AppState) -> axum::Router {
    routes::router(state)
}
