//! Migrations are compiled in via `include_str!` so the single static binary ships no migration files.

use anyhow::Result;
use rusqlite::Connection;
use rusqlite_migration::{Migrations, M};

/// A function rather than a `static` so `include_str!` resolves relative to this file.
fn migrations() -> Migrations<'static> {
    Migrations::new(vec![
        M::up(include_str!("../migrations/V001__initial.sql")),
        // Also created by `yogurt-server::storage::migrations` (`IF NOT EXISTS`). Whichever runner fires first wins and the columns agree.
        M::up(include_str!("../migrations/V002__chat_messages.sql")),
        // `CREATE TABLE IF NOT EXISTS` is a no-op on DBs that already have the older `meetings` table,
        // so [`backfill_phase7_meeting_columns`] adds `starred`, `created_at`, `updated_at` right after.
        M::up(include_str!("../migrations/V003__meetings.sql")),
        // Rebuild `chat_messages` so its FK on `meetings(id)` is ON DELETE CASCADE. The older storage migration declared it without the cascade,
        // and V002's `IF NOT EXISTS` kept that when storage migrated first.
        M::up(include_str!(
            "../migrations/V003b__chat_messages_cascade.sql"
        )),
        // FTS5 keyword search across title, notes_md, transcript_text. Triggers keep the index in sync,
        // but `transcript_text` is maintained explicitly by MeetingRepo::patch because the triggers only mirror base columns.
        M::up(include_str!("../migrations/V004__meetings_fts.sql")),
        // Seed `stt.provider` (cloud) and `stt.model` (small.en) so `meetings/start.rs` can pick an STT adapter without a null check.
        M::up(include_str!("../migrations/V005__stt_provider_model.sql")),
        // Per-meeting STT engine provenance. Nullable so old rows read back as None.
        M::up(include_str!("../migrations/V006__meetings_stt_engine.sql")),
        // Meeting labels: `labels` plus a `meeting_labels` join table that cascades on either side.
        M::up(include_str!("../migrations/V007__labels.sql")),
        // Per-meeting LLM provenance: the model that produced `enriched_md`. Nullable.
        M::up(include_str!("../migrations/V008__meetings_llm_model.sql")),
        // `providers.adapter` ('http' | 'cli'): a row can be driven by a local agent CLI instead of an HTTP endpoint.
        M::up(include_str!("../migrations/V009__providers_adapter.sql")),
        // `providers.cli_model`, the `--model` override for a `cli`-adapter row.
        M::up(include_str!("../migrations/V010__providers_cli_model.sql")),
        // `meetings.template`, the enhance note format, picked by the model or forced by the user.
        M::up(include_str!("../migrations/V011__meetings_template.sql")),
        // `labels.position` (custom sidebar order) and `labels.updated_at` (last-updated order).
        M::up(include_str!(
            "../migrations/V012__labels_position_updated_at.sql"
        )),
    ])
}

/// Idempotent. After V003, adds `starred`, `created_at`, `updated_at` to `meetings` where an older storage migration created the table without them.
/// SQLite has no `ADD COLUMN IF NOT EXISTS`, so the existence check happens in Rust.
pub fn run(conn: &mut Connection) -> Result<()> {
    migrations().to_latest(conn)?;
    backfill_phase7_meeting_columns(conn)?;
    Ok(())
}

fn backfill_phase7_meeting_columns(conn: &Connection) -> Result<()> {
    let existing = existing_columns(conn, "meetings")?;
    // Defaults match the V003 CREATE TABLE so fresh and back-filled DBs end up with identical schemas.
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
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_meetings_starred \
         ON meetings(starred) WHERE starred = 1",
    )?;
    // `notes_md` / `transcript_json` were nullable in the older schema. No backfill is needed: MeetingRepo always writes a value and coerces NULL to "" / "[]" at read time.
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
