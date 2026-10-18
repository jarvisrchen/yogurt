//! Resolved independently of `yogurt-server::storage::default_db_path` so neither crate depends on the other; keep the two in sync if either moves.

use anyhow::{anyhow, Result};
use std::path::PathBuf;

/// Creates the directory if missing.
/// Uses `directories::BaseDirs::home_dir()` because the `dirs` crate is unmaintained.
pub fn yogurt_dir() -> Result<PathBuf> {
    let base = directories::BaseDirs::new()
        .ok_or_else(|| anyhow!("could not resolve user home directory"))?;
    let path = base.home_dir().join(".yogurt");
    std::fs::create_dir_all(&path).map_err(|e| anyhow!("creating {}: {e}", path.display()))?;
    Ok(path)
}

/// Default SQLite path: `~/.yogurt/db.sqlite`.
pub fn db_path() -> Result<PathBuf> {
    Ok(yogurt_dir()?.join("db.sqlite"))
}

pub fn keys_path() -> Result<PathBuf> {
    Ok(yogurt_dir()?.join("keys.json"))
}
