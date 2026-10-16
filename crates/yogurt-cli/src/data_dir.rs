//! Resolves the directory holding yogurt's SQLite database, so `--data-dir` / `$YOGURT_DATA_DIR` can give a worktree instance its own copy.
//! Only the database moves. `yogurt-server::storage` and `yogurt-db` both migrate `db.sqlite`, so a branch carrying a migration would otherwise upgrade the real database under the main binary.
//! Keys, models, and notes stay under `~/.yogurt`.
//!
//! `start`, `doctor`, and `ctl meeting`'s local-DB fallback all call [`resolve`] so they agree on the location.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// `--data-dir` beats `$YOGURT_DATA_DIR`. Pure so it is testable without touching the process env.
fn precedence(flag: Option<PathBuf>, env: Option<PathBuf>) -> Option<PathBuf> {
    flag.or(env)
}

/// Creates the directory if missing. `None` means no override.
pub fn resolve(flag: Option<PathBuf>) -> Result<Option<PathBuf>> {
    let env = std::env::var_os("YOGURT_DATA_DIR").map(PathBuf::from);
    let Some(dir) = precedence(flag, env) else {
        return Ok(None);
    };
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("creating data dir {}", dir.display()))?;
    Ok(Some(dir))
}

/// `RunConfig::db_path` and `app_db_path` point at this same file, matching the production default.
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
