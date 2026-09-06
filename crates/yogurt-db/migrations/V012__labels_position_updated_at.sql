-- yogurt-db schema V012 - UI-12 sidebar label ordering.
--
-- `position` backs the "Custom" sort mode (set via the reorder endpoint).
-- Every existing row defaults to 0, so until a user actually drags
-- something, Custom reads alphabetically via the name tie-break in
-- `labels::list_with_counts` callers.
-- `updated_at` backs "Last updated" (touched on rename/recolor and when
-- a label is applied to or removed from a meeting - see
-- crates/yogurt-db/src/labels.rs). Backfilled to `created_at` so existing
-- labels don't all read as "updated at the epoch".
ALTER TABLE labels ADD COLUMN position INTEGER NOT NULL DEFAULT 0;
ALTER TABLE labels ADD COLUMN updated_at INTEGER NOT NULL DEFAULT 0;
UPDATE labels SET updated_at = created_at;
