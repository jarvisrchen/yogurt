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
    pub fn init_at(db_path: &Path) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("creating storage parent dir {}", parent.display())
            })?;
        }

        // Open the writer first so migrations run on a connection that owns
        // the DB. WAL + NORMAL synchronous is the recommended pairing for
        // app-local SQLite.
        let mut writer = Connection::open(db_path)
            .with_context(|| format!("opening sqlite at {}", db_path.display()))?;
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

        // Open the read pool. Each connection is marked `query_only=ON` so an
        // accidental write through the read handle is rejected at the DB
        // layer rather than silently violating the single-writer invariant.
        let mut reads = Vec::with_capacity(READ_POOL_SIZE);
        for _ in 0..READ_POOL_SIZE {
            let r = Connection::open(db_path)
                .with_context(|| format!("opening read conn at {}", db_path.display()))?;
            r.pragma_update(None, "query_only", "ON")
                .context("PRAGMA query_only=ON")?;
            r.pragma_update(None, "foreign_keys", "ON")
                .context("PRAGMA foreign_keys=ON")?;
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
