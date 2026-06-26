-- yogurt-db schema V002 (Phase 6, Plan 06-01 — chat messages).
--
-- Adds the `chat_messages` table that backs the in-meeting chat UX. Coexists
-- with `yogurt-server::storage::migrations` (Phase 0), which also declares
-- the same table via `CREATE TABLE IF NOT EXISTS` in the same SQLite file.
-- The two declarations agree on column names and types so whichever runner
-- fires first wins; subsequent runs are no-ops.
--
-- ## FK note
--
-- `meetings(id)` lives in `yogurt-server::storage::migrations` (Phase 0).
-- Both runners apply against the same `~/.yogurt/db.sqlite` file in
-- production, so the FK target exists at runtime. SQLite parses FK syntax
-- without requiring the parent table to be defined at CREATE time, so the
-- declaration here is safe even if yogurt-db happens to migrate first.
-- The `ON DELETE CASCADE` is intentional — when a meeting row is deleted
-- (Phase 9 library cleanup), its chat history must go with it (PRD §9).
--
-- ## CHECK constraint
--
-- The role check is intentionally strict (only `user` | `assistant`). The
-- LLM trait uses string roles to accommodate provider-specific values
-- (`system`, `tool`, `function`), but those never get persisted — they
-- are constructed in-memory by `spawn_stream` and discarded after the
-- request. Persisting only user + assistant keeps replays clean.
CREATE TABLE IF NOT EXISTS chat_messages (
    id          TEXT PRIMARY KEY,
    meeting_id  TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    role        TEXT NOT NULL CHECK (role IN ('user', 'assistant')),
    content     TEXT NOT NULL,
    created_at  INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_chat_meeting
    ON chat_messages(meeting_id, created_at);
