//! CLI-7: resolves the directory holding yogurt's SQLite database
//! (`~/.yogurt/db.sqlite` by default), so `--data-dir` / `$YOGURT_DATA_DIR`
//! can point a worktree instance at its own copy instead of sharing the one
//! the main binary is running against.
//!
//! Scope is the actual hazard only (`docs/.planning/agent-workflow.md`
//! section 4D, D6): `yogurt-server::storage` and `yogurt-db` both migrate
//! `~/.yogurt/db.sqlite` independently ("whichever runner fires first
//! wins" -- `crates/yogurt-db/src/migrations.rs`), so a branch carrying a
//! migration can silently upgrade the real database out from under the
//! main binary. Keys (`~/.yogurt/keys.json`), models, and notes are NOT
//! part of this -- they stay under `~/.yogurt` per the keys-live-in-one-
//! file constraint (AGENTS.md).
//!
//! `start`, `doctor`, and `ctl meeting`'s local-DB read fallback all call
//! [`resolve`] so they can never disagree about where the database lives.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// `--data-dir` beats `$YOGURT_DATA_DIR`. Pure so it's unit-testable
/// without touching the process env (same pattern as
/// `ctl::client::port_precedence`).
fn precedence(flag: Option<PathBuf>, env: Option<PathBuf>) -> Option<PathBuf> {
    flag.or(env)
}

/// Resolve an override for `~/.yogurt`, creating the directory if it
/// doesn't exist yet. `None` means "no override -- caller falls back to
/// its own `~/.yogurt` default."
pub fn resolve(flag: Option<PathBuf>) -> Result<Option<PathBuf>> {
    let env = std::env::var_os("YOGURT_DATA_DIR").map(PathBuf::from);
    let Some(dir) = precedence(flag, env) else {
        return Ok(None);
    };
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("creating data dir {}", dir.display()))?;
    Ok(Some(dir))
}

/// The SQLite file inside an overridden data dir. `RunConfig::db_path`
/// (`yogurt-server::storage`) and `RunConfig::app_db_path` (`yogurt-db`)
/// point at the SAME file when overridden -- mirroring the production
/// default, where both already resolve to `~/.yogurt/db.sqlite`.
pub fn db_path(dir: &Path) -> PathBuf {
    dir.join("db.sqlite")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flag_beats_env() {
        assert_eq!(
            precedence(Some(PathBuf::from("/flag")), Some(PathBuf::from("/env"))),
            Some(PathBuf::from("/flag"))
        );
    }

    #[test]
    fn env_used_when_no_flag() {
        assert_eq!(
            precedence(None, Some(PathBuf::from("/env"))),
            Some(PathBuf::from("/env"))
        );
    }

    #[test]
    fn neither_set_is_none() {
        assert_eq!(precedence(None, None), None);
    }
}
