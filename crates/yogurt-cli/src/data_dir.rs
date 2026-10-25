//! Resolves the directory holding yogurt's SQLite database, so `--data-dir` / `$YOGURT_DATA_DIR` can give a worktree instance its own copy.
//! The database and `keys.json` move. `yogurt-server::storage` and `yogurt-db` both migrate `db.sqlite`, so a branch carrying a migration would otherwise upgrade the real database under the main binary.
//! `keys.json` holds the Yogurt Cloud connection, so a scratch instance must not write it into the shared file and a second instance with its own data dir can act as a separate laptop.
//! Models, notes, and the session token stay under `~/.yogurt`.
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

pub fn keys_path(dir: &Path) -> PathBuf {
    dir.join("keys.json")
}

/// The two paths to name in a warning when a custom data dir has no `keys.json` but the default one exists.
pub fn keys_not_copied(dir: &Path) -> Option<(PathBuf, PathBuf)> {
    let default = directories::BaseDirs::new()?
        .home_dir()
        .join(".yogurt")
        .join("keys.json");
    keys_not_copied_with(dir, default)
}

fn keys_not_copied_with(dir: &Path, default: PathBuf) -> Option<(PathBuf, PathBuf)> {
    let own = keys_path(dir);
    (own != default && !own.exists() && default.exists()).then_some((own, default))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warns_only_when_the_default_keys_exist_and_the_data_dir_has_none() {
        let tmp = std::env::temp_dir().join(format!("yogurt-keys-{}", std::process::id()));
        let (dir, default) = (tmp.join("scratch"), tmp.join("home-keys.json"));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(keys_not_copied_with(&dir, default.clone()), None);
        std::fs::write(&default, "{}").unwrap();
        assert_eq!(
            keys_not_copied_with(&dir, default.clone()),
            Some((dir.join("keys.json"), default.clone()))
        );
        std::fs::write(dir.join("keys.json"), "{}").unwrap();
        assert_eq!(keys_not_copied_with(&dir, default), None);
        std::fs::remove_dir_all(&tmp).unwrap();
    }

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
