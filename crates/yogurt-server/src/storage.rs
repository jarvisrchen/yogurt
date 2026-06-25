//! SQLite storage layer with WAL + dual pool (CONTEXT D-22 / D-23).
//!
//! - One single-writer `Mutex<Connection>` to serialize writes.
//! - A small read-only pool (round-robin) for concurrent reads.
//! - Database lives at `~/.yogurt/db.sqlite` by default.

pub mod migrations;

use anyhow::{Context, Result};
use directories::BaseDirs;
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// Number of read-only connections in the pool.
const READ_POOL_SIZE: usize = 4;

/// SQLite storage handle. Cheaply cloneable via `Arc` because all state is
/// already inside `Arc`s under the hood.
pub struct Storage {
    /// Single-writer connection — wrap-and-lock for any write transaction.
    writer: Arc<Mutex<Connection>>,
    /// Read-only connections. Round-robined via `next_read`.
    reads: Vec<Arc<Mutex<Connection>>>,
    /// Round-robin cursor into `reads`.
    next_read: AtomicUsize,
}

impl Storage {
    /// Initialize storage at the default path (`~/.yogurt/db.sqlite`).
    /// Convenience wrapper for `init_at(default_db_path()?)`.
    pub fn init() -> Result<Self> {
        Self::init_at(&default_db_path()?)
    }

    /// Initialize storage at an arbitrary path. Used by tests with tempdirs.
    /// Creates the parent directory if missing.
    ///
    /// MD-06: the parent dir is created at mode 0700 (Unix) and the SQLite
    /// file is chmod'd to 0600 after creation so that on a multi-user macOS
    /// system another user account cannot read the meeting transcripts +
    /// chat history.
    pub fn init_at(db_path: &Path) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            create_private_dir(parent)
                .with_context(|| format!("creating storage parent dir {}", parent.display()))?;
        }

        // Open the writer first so migrations run on a connection that owns
        // the DB. WAL + NORMAL synchronous is the recommended pairing for
        // app-local SQLite.
        let mut writer = Connection::open(db_path)
            .with_context(|| format!("opening sqlite at {}", db_path.display()))?;

        // MD-06: rusqlite doesn't expose file-mode at open time. Tighten
        // post-creation so the DB is owner-only-readable.
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

        // Open the read pool. HI-02: every connection gets the FULL pragma
        // set (journal_mode + synchronous + foreign_keys + query_only). WAL
        // is persistent on-disk, but applying the pragma per-connection makes
        // each reader correct independently of init-time race conditions and
        // future refactors (e.g., recycling a connection across reopens).
        let mut reads = Vec::with_capacity(READ_POOL_SIZE);
        for _ in 0..READ_POOL_SIZE {
            let r = Connection::open(db_path)
                .with_context(|| format!("opening read conn at {}", db_path.display()))?;
            // Belt-and-suspenders: ensure WAL is observed by the reader even
            // if the writer's pragma somehow didn't persist to the file header.
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

    /// Hand out the single-writer connection. Lock for the duration of a
    /// write transaction; release promptly so other writers can proceed.
    pub fn writer(&self) -> Arc<Mutex<Connection>> {
        Arc::clone(&self.writer)
    }

    /// Hand out one of the read connections (round-robin).
    pub fn read(&self) -> Arc<Mutex<Connection>> {
        let idx = self.next_read.fetch_add(1, Ordering::Relaxed) % self.reads.len();
        Arc::clone(&self.reads[idx])
    }
}

/// Default storage path: `<home>/.yogurt/db.sqlite`.
pub fn default_db_path() -> Result<PathBuf> {
    let base = BaseDirs::new().context("could not resolve home directory")?;
    Ok(base.home_dir().join(".yogurt").join("db.sqlite"))
}

/// Create a directory at mode 0700 (Unix), tightening any pre-existing dir
/// at a looser mode. No-op shim on non-Unix. (MD-06)
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

/// Set a file's mode to 0600 on Unix. No-op on non-Unix. (MD-06)
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
