-- V003b — rebuild `chat_messages` so its FK on `meetings(id)` is
-- ON DELETE CASCADE.
--
-- Why a rebuild instead of an ALTER? SQLite cannot ALTER TABLE ADD CONSTRAINT
-- on an existing table. The canonical workaround is rename → create new →
-- copy rows → drop old → rename. Foreign-key checks are disabled for the
-- duration of the rebuild (`PRAGMA foreign_keys = OFF`) so the temporary
-- mid-transaction state doesn't trip the FK validator; the runner restores
-- the previous setting after the migration.
--
-- The migration runner wraps every M::up() in a transaction, so the
-- rename/create/copy/drop sequence is atomic with respect to readers.

PRAGMA foreign_keys = OFF;

ALTER TABLE chat_messages RENAME TO chat_messages_old;

CREATE TABLE chat_messages (
    id          TEXT PRIMARY KEY,
    meeting_id  TEXT NOT NULL,
    role        TEXT NOT NULL,
    content     TEXT NOT NULL,
    created_at  INTEGER NOT NULL,
    FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
);

-- Copy only rows whose meeting still exists. Any orphan rows from a
-- pre-V003 bug are dropped (they were unreachable anyway).
INSERT INTO chat_messages (id, meeting_id, role, content, created_at)
SELECT id, meeting_id, role, content, created_at
  FROM chat_messages_old
 WHERE meeting_id IN (SELECT id FROM meetings);

DROP TABLE chat_messages_old;

CREATE INDEX IF NOT EXISTS idx_chat_meeting
    ON chat_messages(meeting_id, created_at);

PRAGMA foreign_keys = ON;
