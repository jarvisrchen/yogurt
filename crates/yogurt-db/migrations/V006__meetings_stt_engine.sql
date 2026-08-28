-- yogurt-db schema V006 — per-meeting STT engine provenance.
--
-- Nullable so existing rows (recorded before this migration) read back as
-- NULL / None — the UI falls back to "Local" for those. New recordings get
-- stamped at start via routes.rs `start_meeting` with a value like
-- "local · small.en" or "cloud · nova-3".
ALTER TABLE meetings ADD COLUMN stt_engine TEXT;
