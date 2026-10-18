//! SQLite with WAL: one writer connection plus a round-robin read-only pool.

pub mod migrations;

use anyhow::{Context, Result};
use directories::BaseDirs;
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

const READ_POOL_SIZE: usize = 4;

pub struct Storage {
    writer: Arc<Mutex<Connection>>,
    reads: Vec<Arc<Mutex<Connection>>>,
    next_read: AtomicUsize,
}

impl Storage {
    pub fn init() -> Result<Self> {
        Self::init_at(&default_db_path()?)
    }

    /// The parent dir is 0700 and the file 0600 so another macOS user cannot read transcripts.
    pub fn init_at(db_path: &Path) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            create_private_dir(parent)
                .with_context(|| format!("creating storage parent dir {}", parent.display()))?;
        }

        let mut writer = Connection::open(db_path)
            .with_context(|| format!("opening sqlite at {}", db_path.display()))?;

        // rusqlite cannot set the file mode at open, so tighten afterwards.
        chmod_0600_if_unix(db_path).with_context(|| format!("chmod 0600 {}", db_path.display()))?;
        writer
            .pragma_update(None, "journal_mode", "WAL")
            .context("PRAGMA journal_mode=WAL")?;
        writer
            .pragma_update(None, "synchronous", "NORMAL")
            .context("PRAGMA synchronous=NORMAL")?;
        writer
            .pragma_update(None, "foreign_keys", "ON")
            .context("PRAGMA foreign_keys=ON")?;

        migrations::run(&mut writer).context("running v1 schema migration")?;

        // Every reader gets the full pragma set so it is correct independently of init order.
        let mut reads = Vec::with_capacity(READ_POOL_SIZE);
        for _ in 0..READ_POOL_SIZE {
            let r = Connection::open(db_path)
                .with_context(|| format!("opening read conn at {}", db_path.display()))?;
            r.pragma_update(None, "journal_mode", "WAL")
                .context("PRAGMA journal_mode=WAL (read conn)")?;
            r.pragma_update(None, "synchronous", "NORMAL")
                .context("PRAGMA synchronous=NORMAL (read conn)")?;
            r.pragma_update(None, "foreign_keys", "ON")
                .context("PRAGMA foreign_keys=ON (read conn)")?;
            r.pragma_update(None, "query_only", "ON")
                .context("PRAGMA query_only=ON (read conn)")?;
            reads.push(Arc::new(Mutex::new(r)));
        }

        Ok(Self {
            writer: Arc::new(Mutex::new(writer)),
            reads,
            next_read: AtomicUsize::new(0),
        })
    }

    pub fn writer(&self) -> Arc<Mutex<Connection>> {
        Arc::clone(&self.writer)
    }

    pub fn read(&self) -> Arc<Mutex<Connection>> {
        let idx = self.next_read.fetch_add(1, Ordering::Relaxed) % self.reads.len();
        Arc::clone(&self.reads[idx])
    }
}

pub fn default_db_path() -> Result<PathBuf> {
    let base = BaseDirs::new().context("could not resolve home directory")?;
    Ok(base.home_dir().join(".yogurt").join("db.sqlite"))
}

/// Creates the directory at 0700, tightening a pre-existing looser one.
fn create_private_dir(p: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        let mut b = std::fs::DirBuilder::new();
        b.recursive(true).mode(0o700);
        match b.create(p) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
        use std::os::unix::fs::PermissionsExt;
        let meta = std::fs::metadata(p)?;
        let mode = meta.permissions().mode() & 0o777;
        if mode != 0o700 {
            std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o700))?;
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(p)
    }
}

fn chmod_0600_if_unix(p: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o600))
    }
    #[cfg(not(unix))]
    {
        let _ = p;
        Ok(())
    }
}
