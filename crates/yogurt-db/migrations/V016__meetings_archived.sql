-- Unix ms when the meeting was archived, NULL while it is not. No index: a scan of a few thousand rows is fast,
-- and SQLite cannot use a partial index behind the bound `:archived` the Library filter compares against.
ALTER TABLE meetings ADD COLUMN archived_at INTEGER;
