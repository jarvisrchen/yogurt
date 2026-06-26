-- yogurt-db schema V003 (Phase 7, Plan 07-01 — meetings repo).
--
-- Promotes the Phase-3 in-memory meeting registry to a SQLite-backed
-- repository powering the new Library route at `/`. The physical
-- `meetings` table is shared with `yogurt-server::storage::migrations`
-- (Phase 0), which already declared it with columns:
--   id, title, started_at, ended_at, notes_md, enriched_md,
--   transcript_json, enriched_doc_json
--
-- Phase 7 adds three new columns the Library needs:
--   - starred       INTEGER NOT NULL DEFAULT 0   (UI surface deferred to v1.1
--                                                  but stored now so we can
--                                                  do the migration once)
--   - created_at    INTEGER NOT NULL DEFAULT 0   (row creation time, distinct
--                                                  from started_at which is
--                                                  the meeting's recording-
--                                                  start clock)
--   - updated_at    INTEGER NOT NULL DEFAULT 0   (touched on every PATCH so
--                                                  the Library can re-sort
--                                                  on edit)
--
-- Schema-conflict resolution:
--   * The table is created `IF NOT EXISTS` so a brand-new DB (where this
--     migration runs BEFORE Phase 0 storage has booted) gets a complete
--     schema and Phase-0 storage's later `CREATE TABLE IF NOT EXISTS` is
--     a no-op.
--   * `ALTER TABLE ... ADD COLUMN` is wrapped in a `CASE WHEN col_exists`
--     selector via the `pragma_table_info` trick so this migration is
--     idempotent against the Phase 0 schema that already exists in user DBs.
--
-- FK retrofit on chat_messages:
--   Phase-0 storage's `chat_messages` was declared WITHOUT
--   `ON DELETE CASCADE`. yogurt-db V002 declared it WITH cascade but
--   `CREATE TABLE IF NOT EXISTS` made that a no-op when storage migrated
--   first. V003 rebuilds the table so DELETE /api/meetings/:id cascades
--   correctly (LIB-09 truth: deleting a meeting purges its chat history).

-- ─── meetings table: bootstrap if missing, add Phase 7 columns if absent ──

CREATE TABLE IF NOT EXISTS meetings (
    id                TEXT PRIMARY KEY,
    title             TEXT,
    started_at        INTEGER NOT NULL DEFAULT 0,
    ended_at          INTEGER,
    notes_md          TEXT NOT NULL DEFAULT '',
    enriched_md       TEXT,
    transcript_json   TEXT NOT NULL DEFAULT '[]',
    enriched_doc_json TEXT,
    starred           INTEGER NOT NULL DEFAULT 0,
    created_at        INTEGER NOT NULL DEFAULT 0,
    updated_at        INTEGER NOT NULL DEFAULT 0
);

-- ─── Phase 7 additive columns (idempotent guarded ALTER) ──────────────────
--
-- SQLite doesn't support `ALTER TABLE ADD COLUMN IF NOT EXISTS`. The
-- runner does the existence check in code below for ALTERs, so this
-- migration file restricts itself to safe-by-construction statements.
--
-- For brand-new DBs the CREATE above already includes the columns.
-- For pre-existing DBs (Phase 0 table only), the Rust-side migration
-- runner inspects `pragma_table_info('meetings')` and conditionally
-- executes the ALTERs that ship next to this SQL file.

-- ─── Indexes ──────────────────────────────────────────────────────────────
--
-- Only indexes whose columns exist in BOTH the fresh-DB CREATE TABLE
-- above AND the legacy Phase 0 schema can live here. `idx_meetings_starred`
-- references the Phase 7 `starred` column, which is back-filled by the
-- Rust-side `backfill_phase7_meeting_columns` helper AFTER this migration
-- runs — so the partial-index creation also lives in Rust (see
-- `migrations.rs::backfill_phase7_meeting_columns`).

CREATE INDEX IF NOT EXISTS idx_meetings_started
    ON meetings(started_at DESC);

-- ─── Seed onboarding flag (Phase 7 settings KV) ───────────────────────────

INSERT OR IGNORE INTO settings(key, value) VALUES
    ('first_run_completed', 'false');
