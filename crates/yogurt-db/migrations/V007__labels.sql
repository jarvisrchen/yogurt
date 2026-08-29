-- yogurt-db schema V007 — Granola-style meeting labels.
--
-- `labels` is a workspace-level named-tag table (name unique, case-insensitive).
-- `meeting_labels` is the many-to-many join, cascade-deleted on either side
-- so removing a label or a meeting cleans up automatically (foreign_keys
-- pragma is already ON in Db::open / open_in_memory).
CREATE TABLE IF NOT EXISTS labels (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    color      TEXT NOT NULL,
    created_at INTEGER NOT NULL DEFAULT 0
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_labels_name_nocase ON labels(name COLLATE NOCASE);
CREATE TABLE IF NOT EXISTS meeting_labels (
    meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    label_id   TEXT NOT NULL REFERENCES labels(id)   ON DELETE CASCADE,
    PRIMARY KEY (meeting_id, label_id)
);
CREATE INDEX IF NOT EXISTS idx_meeting_labels_label ON meeting_labels(label_id);
