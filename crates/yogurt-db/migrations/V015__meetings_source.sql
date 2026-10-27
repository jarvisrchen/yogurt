-- Where a meeting came from. NULL for meetings recorded in yogurt, 'granola' for imports.
ALTER TABLE meetings ADD COLUMN source TEXT;
-- The source's own id for the meeting (Granola's not_ id).
ALTER TABLE meetings ADD COLUMN source_id TEXT;
-- Display-only provenance: {"owner":{"name","email"},"web_url","updated_at","imported_at"}.
ALTER TABLE meetings ADD COLUMN source_json TEXT;
-- Makes a re-run of an import a no-op for meetings it already brought in.
CREATE UNIQUE INDEX idx_meetings_source ON meetings(source, source_id) WHERE source IS NOT NULL;
