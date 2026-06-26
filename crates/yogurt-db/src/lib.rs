//! `yogurt-db` — SQLite (providers + settings) + macOS Keychain wrapper.
//!
//! Owned by Phase 5, Plan 05-02. Coexists with the Phase 0
//! `yogurt-server::storage` module by managing a disjoint set of tables
//! (`providers`, `settings`) in the same `~/.yogurt/db.sqlite` file.
//!
//! ## Modules
//! - [`paths`] — filesystem path resolution (`~/.yogurt/`, `db.sqlite`).
//! - [`providers`] — CRUD + preset definitions + atomic active-flag toggle.
//! - [`settings`] — typed `General` settings backed by the KV table.
//! - [`keychain`] — `ApiKeyStore` trait + `KeychainStore` + `MemoryKeyStore`.
//!
//! ## Concurrency
//! `rusqlite::Connection` is not `Sync`, so `Db` wraps it in
//! `Arc<Mutex<Connection>>`. Single-user, ~ms-latency queries — contention is
//! negligible at this phase. Phase 9 may swap in a pool if needed.

mod migrations;

pub mod chat;
pub mod keychain;
pub mod meetings;
pub mod paths;
pub mod providers;
pub mod settings;

// Phase 7 (Plan 07-01): crate-root re-exports for the new MeetingRepo so
// server-side handlers can `use yogurt_db::{Meeting, MeetingRepo, ...}`
// without reaching into the submodule path.
pub use meetings::{Meeting, MeetingPatch, MeetingRepo, NewMeeting};

use anyhow::Result;
use rusqlite::Connection;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// Cloneable, thread-safe handle to the SQLite database.
#[derive(Clone)]
pub struct Db {
    inner: Arc<Mutex<Connection>>,
}

impl Db {
    /// Open a SQLite database at `path`, set `journal_mode=WAL` + foreign keys,
    /// and run migrations.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        migrations::run(&mut conn)?;
        Ok(Self {
            inner: Arc::new(Mutex::new(conn)),
        })
    }

    /// Open the default path (`~/.yogurt/db.sqlite`).
    pub fn open_default() -> Result<Self> {
        Self::open(paths::db_path()?)
    }

    /// Open an in-memory database with migrations applied. For tests.
    pub fn open_in_memory() -> Result<Self> {
        let mut conn = Connection::open_in_memory()?;
        migrations::run(&mut conn)?;
        Ok(Self {
            inner: Arc::new(Mutex::new(conn)),
        })
    }

    /// Re-run migrations against the underlying connection. Idempotent;
    /// exposed for tests to verify migration safety.
    pub fn run_migrations(&self) -> Result<()> {
        let mut conn = self.inner.lock().expect("db mutex poisoned");
        migrations::run(&mut conn)
    }

    /// Acquire the connection guard for a closure-based query. Prefer the
    /// module-level helpers (`providers::list`, `settings::get`, ...) over
    /// reaching into `with_conn` directly.
    pub fn with_conn<R>(&self, f: impl FnOnce(&Connection) -> R) -> R {
        let guard = self.inner.lock().expect("db mutex poisoned");
        f(&guard)
    }

    /// Test-only convenience accessor for direct SQL inspection.
    #[doc(hidden)]
    pub fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.inner.lock().expect("db mutex poisoned")
    }
}
