//! Shared per-request `AppState`.
//!
//! Phase 5 (Plan 05-02) extends Phase 4's `AppState` with two new fields:
//! - [`db`](AppState::db) — the new [`yogurt_db::Db`] (providers + settings
//!   tables in the same `~/.yogurt/db.sqlite` file as Phase 0 `storage`).
//! - [`keys`](AppState::keys) — an `Arc<dyn ApiKeyStore>` over the macOS
//!   Keychain (`KeychainStore` in production, `MemoryKeyStore` in tests).
//!
//! ## SET-10 cold-boot mitigation
//!
//! `keyring::Entry::get_password()` synchronously prompts the user on first
//! Keychain access. Request handlers MUST NEVER call it directly during a
//! request — they would block the tokio reactor for seconds while waiting
//! for the Keychain daemon (or the user) to respond.
//!
//! The mitigation is `production_warmed()`: at server startup we pre-walk
//! every provider and call `keys.get(id)` once inside `spawn_blocking`,
//! wrapped in a 5-second `tokio::time::timeout`. The blocking call lives
//! off the tokio scheduler; the timeout bounds cold-boot regardless of
//! Keychain daemon state. On timeout we log a warning and continue with
//! an empty warm-up — request handlers still work (they just hit cold
//! Keychain on demand, which is the pre-mitigation behavior).

use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use yogurt_db::keychain::{ApiKeyStore, KeychainStore, MemoryKeyStore};
use yogurt_db::Db;

use crate::markdown_exporter::MarkdownExporter;
use crate::meetings;
use crate::session::SessionToken;
use crate::storage::Storage;
use crate::Mode;

/// Shared per-request app state.
///
/// Phase 4 fields (storage, session, meetings, markdown_exporter, prompts)
/// are preserved verbatim. Phase 5 adds `db` and `keys`.
#[derive(Clone)]
pub struct AppState {
    pub mode: Mode,
    pub storage: Arc<Storage>,
    pub session: Arc<SessionToken>,
    pub bind_port: u16,
    /// Phase 3: in-memory meeting registry.
    pub meetings: Arc<meetings::Registry>,
    /// Phase 4 (Plan 04-03): single-writer markdown file emitter.
    pub markdown_exporter: Arc<MarkdownExporter>,
    /// Phase 4 (Plan 04-03): bundled prompt templates.
    pub prompts: Arc<yogurt_prompts::Prompts>,
    /// Phase 5 (Plan 05-02): SQLite-backed providers + settings tables.
    pub db: Db,
    /// Phase 5 (Plan 05-02): API-key storage abstraction. `KeychainStore`
    /// in production, `MemoryKeyStore` in tests.
    pub keys: Arc<dyn ApiKeyStore>,
}

/// Configuration for production constructors. Mirrors the field set of
/// `crate::RunConfig` so the top-level `run_with_config` can hand it through
/// without reshuffling.
pub struct ProductionConfig {
    pub mode: Mode,
    pub bind_port: u16,
    pub storage: Arc<Storage>,
    pub session: Arc<SessionToken>,
    pub notes_dir: PathBuf,
}

impl AppState {
    /// Build the Phase 5 `AppState` for production:
    /// - `Db::open_default()` opens `~/.yogurt/db.sqlite` (shared with
    ///   Phase 0 storage; tables are disjoint).
    /// - `KeychainStore` reads real macOS Keychain entries on demand.
    ///
    /// Does NOT warm up the Keychain — call [`Self::production_warmed`]
    /// for the warmed variant.
    pub fn production(cfg: ProductionConfig) -> Result<Self> {
        let exporter = Arc::new(MarkdownExporter::new(cfg.notes_dir)?);
        let prompt_mode = match cfg.mode {
            Mode::Dev => yogurt_prompts::Mode::Dev,
            Mode::Release => yogurt_prompts::Mode::Release,
        };
        let prompts = Arc::new(yogurt_prompts::Prompts::load(prompt_mode)?);
        Ok(Self {
            mode: cfg.mode,
            storage: cfg.storage,
            session: cfg.session,
            bind_port: cfg.bind_port,
            meetings: meetings::Registry::new(),
            markdown_exporter: exporter,
            prompts,
            db: Db::open_default()?,
            // Phase 5 BLOCKER fix: KeychainStore::new() registers the
            // apple-native-keyring-store backend with keyring-core. The
            // unit-struct form silently no-op'd set_password() under
            // keyring 3.6.x on macOS in 2026.
            keys: Arc::new(KeychainStore::new()?),
        })
    }

    /// Build the Phase 5 `AppState` for production AND warm the Keychain.
    ///
    /// Bounded by `tokio::time::timeout(Duration::from_secs(5), ...)` per
    /// SET-10. On timeout the warm-up is abandoned but the state is still
    /// returned — request handlers fall back to on-demand `keys.get`.
    pub async fn production_warmed(cfg: ProductionConfig) -> Result<Self> {
        let state = Self::production(cfg)?;
        warm_keychain(&state).await;
        Ok(state)
    }

    /// Test wiring: real Phase 0 storage at `storage_path`, in-memory
    /// yogurt-db, and a `MemoryKeyStore`. Intended for `tests/bootstrap.rs`
    /// and similar suites that need the new `db` + `keys` surface without
    /// touching the real Keychain.
    pub fn in_memory(
        mode: Mode,
        storage: Arc<Storage>,
        session: Arc<SessionToken>,
        bind_port: u16,
        notes_dir: PathBuf,
    ) -> Result<Self> {
        let exporter = Arc::new(MarkdownExporter::new(notes_dir)?);
        let prompt_mode = match mode {
            Mode::Dev => yogurt_prompts::Mode::Dev,
            Mode::Release => yogurt_prompts::Mode::Release,
        };
        let prompts = Arc::new(yogurt_prompts::Prompts::load(prompt_mode)?);
        Ok(Self {
            mode,
            storage,
            session,
            bind_port,
            meetings: meetings::Registry::new(),
            markdown_exporter: exporter,
            prompts,
            db: Db::open_in_memory()?,
            keys: Arc::new(MemoryKeyStore::default()),
        })
    }
}

/// Warm the Keychain cache by walking every provider once. SET-10:
///
/// 1. Read provider IDs from the (cheap) in-memory SQLite. If this fails
///    (corrupt DB, etc.) we log and skip — startup must not abort here.
/// 2. Hand the bulk read off the tokio scheduler via `spawn_blocking` —
///    `keyring` is a synchronous C-FFI call that can hang for seconds.
/// 3. Wrap the whole job in a 5-second `timeout`. On expiry, log warn
///    and continue. The cache is best-effort; correctness does not
///    depend on it succeeding.
async fn warm_keychain(state: &AppState) {
    let providers = match yogurt_db::providers::list(&state.db) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = %e, "warm_keychain: failed to list providers; skipping");
            return;
        }
    };
    if providers.is_empty() {
        // Fresh install — nothing to warm, but the path still completes
        // inside the timeout to satisfy the cold-boot test.
        return;
    }
    let keys = state.keys.clone();
    let warm = tokio::task::spawn_blocking(move || {
        for p in providers {
            // We discard the result intentionally — the only goal here is
            // to prompt the OS to cache the entry. Real reads happen
            // on-demand in the handlers.
            let _ = keys.get(&p.id);
        }
    });
    match tokio::time::timeout(Duration::from_secs(5), warm).await {
        Ok(Ok(())) => {
            tracing::debug!("warm_keychain: pre-warm complete");
        }
        Ok(Err(join_err)) => {
            tracing::warn!(error = %join_err, "warm_keychain: spawn_blocking join failed");
        }
        Err(_elapsed) => {
            tracing::warn!(
                "warm_keychain: timed out after 5s; continuing with cold keychain (SET-10)"
            );
        }
    }
}
