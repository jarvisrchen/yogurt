//! Embedded migrations for the yogurt-db crate.
//!
//! Migrations are compiled into the binary via `include_str!` so the single
//! static-binary distribution model holds (no on-disk migration files
//! shipped). Phase 6 adds V002 for additional tables.

use anyhow::Result;
use rusqlite::Connection;
use rusqlite_migration::{Migrations, M};

/// Build the migration set. Held as a function (not a `static`) so
/// `include_str!` resolves relative to this file at compile time.
fn migrations() -> Migrations<'static> {
    Migrations::new(vec![M::up(include_str!("../migrations/V001__initial.sql"))])
}

/// Run pending migrations on `conn` (idempotent — safe to call repeatedly).
pub fn run(conn: &mut Connection) -> Result<()> {
    migrations().to_latest(conn)?;
    Ok(())
}
