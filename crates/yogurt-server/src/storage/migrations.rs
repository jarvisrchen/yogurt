//! v1 schema migration (Phase 0 scope per CONTEXT D-23).
//!
//! Creates the `meetings` and `chat_messages` tables plus their indexes.
//! All statements are idempotent (`CREATE TABLE IF NOT EXISTS` /
//! `CREATE INDEX IF NOT EXISTS`) so a second `run` against the same DB is a
//! no-op.
//!
//! IMPORTANT: the `enriched_doc_json TEXT` column on `meetings` is **deferred
//! to Phase 4** per the REQUIREMENTS.md STORE-01 split mapping. Do not add it
//! here.

use rusqlite::Connection;

/// Run the v1 schema migration. Idempotent.
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
    tx.commit()?;
    Ok(())
}
