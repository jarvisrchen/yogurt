//! Filesystem paths for the yogurt SQLite + assets directory.
//!
//! Phase 0's `yogurt-server::storage::default_db_path` resolves the same
//! `~/.yogurt/db.sqlite` path with the same `directories` crate. The two
//! modules are intentionally independent so neither crate depends on the
//! other for path resolution — keep them in sync if either ever moves.

use anyhow::{anyhow, Result};
use std::path::PathBuf;

/// Returns `~/.yogurt/` and creates the directory if missing.
///
/// Uses `directories::BaseDirs::home_dir()` per the PRD; do not switch to
/// `dirs::home_dir()` (the `dirs` crate is unmaintained).
pub fn yogurt_dir() -> Result<PathBuf> {
    let base = directories::BaseDirs::new()
        .ok_or_else(|| anyhow!("could not resolve user home directory"))?;
    let path = base.home_dir().join(".yogurt");
    std::fs::create_dir_all(&path).map_err(|e| anyhow!("creating {}: {e}", path.display()))?;
    Ok(path)
}

/// Default SQLite path: `~/.yogurt/db.sqlite`. Shared with Phase 0 storage.
pub fn db_path() -> Result<PathBuf> {
    Ok(yogurt_dir()?.join("db.sqlite"))
}
