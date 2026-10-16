//! SQLite store plus the API-key file store.
//! Shares `~/.yogurt/db.sqlite` with `yogurt-server::storage`, each managing a disjoint set of tables.
//! `rusqlite::Connection` is not `Sync`, so `Db` wraps it in `Arc<Mutex<Connection>>`.

mod migrations;

pub mod chat;
pub mod keys;
pub mod labels;
pub mod meetings;
pub mod paths;
pub mod providers;
pub mod settings;

pub use labels::{Label, LabelRepo, LabelWithCount};
pub use meetings::{Meeting, MeetingPatch, MeetingRepo, NewMeeting};

use anyhow::Result;
use rusqlite::Connection;
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct Db {
    inner: Arc<Mutex<Connection>>,
}

impl Db {
    /// Sets `journal_mode=WAL` and foreign keys, then runs migrations.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        migrations::run(&mut conn)?;
        Ok(Self {
            inner: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn open_default() -> Result<Self> {
        Self::open(paths::db_path()?)
    }

    pub fn open_in_memory() -> Result<Self> {
        let mut conn = Connection::open_in_memory()?;
        migrations::run(&mut conn)?;
        Ok(Self {
            inner: Arc::new(Mutex::new(conn)),
        })
    }

    /// Idempotent; exposed for tests.
    pub fn run_migrations(&self) -> Result<()> {
        let mut conn = self.inner.lock().expect("db mutex poisoned");
        migrations::run(&mut conn)
    }

    /// Prefer the module-level helpers (`providers::list`, `settings::get`, ...) over reaching in here.
    pub fn with_conn<R>(&self, f: impl FnOnce(&Connection) -> R) -> R {
        let guard = self.inner.lock().expect("db mutex poisoned");
        f(&guard)
    }

    #[doc(hidden)]
    pub fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.inner.lock().expect("db mutex poisoned")
    }
}
