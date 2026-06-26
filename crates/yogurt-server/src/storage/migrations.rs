//! v1 schema migration (Phase 0 scope per CONTEXT D-23).
//!
//! Creates the `meetings` and `chat_messages` tables plus their indexes.
//! All statements are idempotent (`CREATE TABLE IF NOT EXISTS` /
//! `CREATE INDEX IF NOT EXISTS`) so a second `run` against the same DB is a
//! no-op.
//!
//! Phase 4 (CONTEXT D-11) adds `enriched_doc_json TEXT` to `meetings` via
//! `V0004__add_enriched_doc_json.sql`. SQLite's `ALTER TABLE` lacks an
//! `IF NOT EXISTS` clause so we inspect `PRAGMA table_info` first and
//! skip the ALTER when the column is already present. The SQL itself
//! lives in `migrations/V0004__add_enriched_doc_json.sql` so it can be
//! grepped + audited as a file, and is `include_str!`ed below to keep
//! all schema in the binary (single-file distribution).

use rusqlite::Connection;

/// SQL for Phase 4's enriched_doc_json column addition (STORE-01).
const V0004_ADD_ENRICHED_DOC_JSON: &str =
    include_str!("migrations/V0004__add_enriched_doc_json.sql");

/// Run the v1 schema migration (Phase 0) + the Phase 4 additive migration.
/// Idempotent.
pub fn run(conn: &mut Connection) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    tx.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS meetings (
            id TEXT PRIMARY KEY,
            title TEXT,
            started_at INTEGER NOT NULL,
            ended_at INTEGER,
            notes_md TEXT,
            enriched_md TEXT,
            transcript_json TEXT
        );

        CREATE TABLE IF NOT EXISTS chat_messages (
            id TEXT PRIMARY KEY,
            meeting_id TEXT NOT NULL,
            role TEXT NOT NULL,
            content TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            FOREIGN KEY(meeting_id) REFERENCES meetings(id)
        );

        CREATE INDEX IF NOT EXISTS idx_meetings_started_at
            ON meetings(started_at DESC);

        CREATE INDEX IF NOT EXISTS idx_chat_messages_meeting_id
            ON chat_messages(meeting_id, created_at);
        "#,
    )?;

    // Phase 4: additive ALTER guarded by a PRAGMA check (SQLite has no
    // ALTER ... IF NOT EXISTS). The check must run inside the same
    // transaction so concurrent boots see a consistent state.
    if !column_exists(&tx, "meetings", "enriched_doc_json")? {
        tx.execute_batch(V0004_ADD_ENRICHED_DOC_JSON)?;
    }

    tx.commit()?;
    Ok(())
}

/// Return whether `column` is present on `table` according to `PRAGMA
/// table_info`. Used to make additive `ALTER TABLE ... ADD COLUMN`
/// migrations idempotent (SQLite doesn't support `IF NOT EXISTS` on
/// ALTER).
fn column_exists(
    tx: &rusqlite::Transaction<'_>,
    table: &str,
    column: &str,
) -> rusqlite::Result<bool> {
    // PRAGMA table_info doesn't accept bound parameters in older SQLite
    // builds. We hand-validate `table` because it is an identifier, not
    // user input — these callers pass static string literals.
    let sql = format!("PRAGMA table_info({table})");
    let mut stmt = tx.prepare(&sql)?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let name: String = row.get(1)?;
        if name == column {
            return Ok(true);
        }
    }
    Ok(false)
}
