//! Embedded migrations for the yogurt-db crate.
//!
//! Migrations are compiled into the binary via `include_str!` so the single
//! static-binary distribution model holds (no on-disk migration files
//! shipped). Phase 6 adds V002; Phase 7 adds V003 (meetings repo + FK
//! retrofit on chat_messages).

use anyhow::Result;
use rusqlite::Connection;
use rusqlite_migration::{Migrations, M};

/// Build the migration set. Held as a function (not a `static`) so
/// `include_str!` resolves relative to this file at compile time.
fn migrations() -> Migrations<'static> {
    Migrations::new(vec![
        M::up(include_str!("../migrations/V001__initial.sql")),
        // Phase 6 (Plan 06-01): chat_messages table for in-meeting chat.
        // Coexists with `yogurt-server::storage::migrations`' identical
        // `CREATE TABLE IF NOT EXISTS chat_messages` (whichever runner fires
        // first wins; the schemas agree at the column level — the FK
        // cascade gap is closed in V003b).
        M::up(include_str!("../migrations/V002__chat_messages.sql")),
        // Phase 7 (Plan 07-01): promote the Phase-3 in-memory meeting
        // registry to a SQLite-backed repo powering the Library route.
        // The CREATE TABLE IF NOT EXISTS is a no-op on user DBs that
        // already have the Phase 0 `meetings` table — the column
        // backfill for `starred`, `created_at`, `updated_at` happens in
        // [`backfill_phase7_meeting_columns`] which runs immediately
        // after this migration.
        M::up(include_str!("../migrations/V003__meetings.sql")),
        // Phase 7 (Plan 07-01) part B: rebuild `chat_messages` so its FK
        // on `meetings(id)` is ON DELETE CASCADE. Phase-0 storage
        // declared the table without the cascade, and yogurt-db V002's
        // `CREATE TABLE IF NOT EXISTS` made the cascade declaration a
        // no-op when storage migrated first. Rebuild fixes that.
        M::up(include_str!(
            "../migrations/V003b__chat_messages_cascade.sql"
        )),
        // Phase 7 (Plan 07-02): FTS5 keyword search across title +
        // notes_md + transcript_text (LIB-07). The triggers in the SQL
        // keep the index in sync on INSERT/UPDATE/DELETE; the
        // `transcript_text` column is explicitly maintained by
        // MeetingRepo::patch because the trigger only knows how to
        // mirror the base columns and would otherwise leave the
        // searchable transcript blank.
        M::up(include_str!("../migrations/V004__meetings_fts.sql")),
        // Phase 8 (Plan 08-03): seed `stt.provider` + `stt.model` KV rows
        // so `meetings/start.rs` can branch between the Deepgram cloud
        // adapter and the WhisperLocal adapter without a settings null
        // check.  Defaults: provider=cloud, model=small.en (D-02).
        M::up(include_str!("../migrations/V005__stt_provider_model.sql")),
        // Feature: per-meeting STT engine provenance. Nullable TEXT so old
        // rows read back as None; new recordings get stamped in
        // routes.rs::start_meeting.
        M::up(include_str!("../migrations/V006__meetings_stt_engine.sql")),
        // Feature: Granola-style meeting labels. `labels` (workspace-level
        // named tags) + `meeting_labels` (many-to-many join, cascades on
        // either side).
        M::up(include_str!("../migrations/V007__labels.sql")),
        // Feature: per-meeting LLM provenance. Nullable TEXT stamped by
        // enhance.rs with the model that produced `enriched_md`.
        M::up(include_str!("../migrations/V008__meetings_llm_model.sql")),
    ])
}

/// Run pending migrations on `conn` (idempotent — safe to call repeatedly).
///
/// Phase 7 collateral: after the V003 migration creates / harmonizes the
/// `meetings` schema, run a code-driven backfill that adds the three new
/// Phase 7 columns (`starred`, `created_at`, `updated_at`) to any user DB
/// where the table was originally created by Phase 0 storage (which
/// predates those columns). SQLite has no `ALTER TABLE ADD COLUMN IF NOT
/// EXISTS`, so the existence check happens in Rust here and the ALTER is
/// skipped when the column is already present.
pub fn run(conn: &mut Connection) -> Result<()> {
    migrations().to_latest(conn)?;
    backfill_phase7_meeting_columns(conn)?;
    Ok(())
}

/// Phase 7 (Plan 07-01): add `starred`, `created_at`, `updated_at` to the
/// `meetings` table if they are missing. Idempotent — re-running over a DB
/// that already has the columns is a no-op.
fn backfill_phase7_meeting_columns(conn: &Connection) -> Result<()> {
    let existing = existing_columns(conn, "meetings")?;
    // The defaults match the V003 CREATE TABLE statement above so a
    // fresh DB (which already has the columns from CREATE) and a
    // back-filled DB (where ALTER runs here) end up with identical
    // schemas.
    if !existing.contains(&"starred".to_string()) {
        conn.execute_batch("ALTER TABLE meetings ADD COLUMN starred INTEGER NOT NULL DEFAULT 0")?;
    }
    if !existing.contains(&"created_at".to_string()) {
        conn.execute_batch(
            "ALTER TABLE meetings ADD COLUMN created_at INTEGER NOT NULL DEFAULT 0",
        )?;
    }
    if !existing.contains(&"updated_at".to_string()) {
        conn.execute_batch(
            "ALTER TABLE meetings ADD COLUMN updated_at INTEGER NOT NULL DEFAULT 0",
        )?;
    }
    // Index on the now-present `starred` column. The CREATE INDEX is
    // idempotent thanks to IF NOT EXISTS — runs on every boot but never
    // duplicates the index.
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_meetings_starred \
         ON meetings(starred) WHERE starred = 1",
    )?;
    // notes_md / transcript_json default-tightening: the Phase 0 schema
    // declared these as nullable; the Phase 7 repo treats them as
    // non-null. New writes go through MeetingRepo which always supplies
    // a value, so no backfill query is needed for legacy rows — the
    // repo coerces SQL NULL → "" / "[]" at read time.
    Ok(())
}

fn existing_columns(conn: &Connection, table: &str) -> Result<Vec<String>> {
    let sql = format!("PRAGMA table_info({table})");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(1))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}
