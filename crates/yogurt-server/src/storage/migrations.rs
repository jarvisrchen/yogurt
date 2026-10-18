//! v1 schema migration. Every statement is idempotent, so a second `run` is a no-op.
//!
//! SQLite's `ALTER TABLE` has no `IF NOT EXISTS`, so the additive `enriched_doc_json` column is
//! guarded by a `PRAGMA table_info` check inside the same transaction.

use rusqlite::Connection;

const V0004_ADD_ENRICHED_DOC_JSON: &str =
    include_str!("migrations/V0004__add_enriched_doc_json.sql");

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

    if !column_exists(&tx, "meetings", "enriched_doc_json")? {
        tx.execute_batch(V0004_ADD_ENRICHED_DOC_JSON)?;
    }

    tx.commit()?;
    Ok(())
}

fn column_exists(
    tx: &rusqlite::Transaction<'_>,
    table: &str,
    column: &str,
) -> rusqlite::Result<bool> {
    // PRAGMA table_info takes no bound parameters, so `table` must be a static literal.
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
