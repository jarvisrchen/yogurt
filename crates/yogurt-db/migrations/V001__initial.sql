-- yogurt-db schema V001 (Phase 5, Plan 05-02 — providers + settings only).
--
-- This migration is owned by the `yogurt-db` crate. It coexists with
-- `yogurt-server::storage::migrations` (Phase 0), which owns the disjoint
-- `meetings` + `chat_messages` tables in the same `~/.yogurt/db.sqlite` file.
-- The two runners are independent; do not introduce table-name overlap.
--
-- Phase 6 will add a V002 here for chat-related additions.

CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS providers (
    id            TEXT PRIMARY KEY,           -- ulid
    name          TEXT NOT NULL,              -- e.g. "Minimax", "OpenAI"
    base_url      TEXT NOT NULL,              -- e.g. "https://api.minimax.io/v1"
    model         TEXT NOT NULL DEFAULT '',   -- e.g. "MiniMax-Text-01"
    kind          TEXT NOT NULL DEFAULT 'llm',-- 'llm' | (future: 'stt')
    is_active     INTEGER NOT NULL DEFAULT 0, -- bool; at most one row per kind
    created_at    INTEGER NOT NULL            -- unix millis
);

-- Partial unique index: at most one active provider per kind. This is the
-- DB-layer enforcement of the "single active LLM" invariant (PRD §5.6, §9).
CREATE UNIQUE INDEX IF NOT EXISTS idx_providers_one_active_per_kind
    ON providers(kind) WHERE is_active = 1;

CREATE INDEX IF NOT EXISTS idx_providers_kind ON providers(kind);

-- Seed default general settings. `INSERT OR IGNORE` keeps re-runs idempotent.
INSERT OR IGNORE INTO settings(key, value) VALUES
    ('general.port', '7878'),
    ('general.open_browser_on_start', 'true'),
    ('audio.input_device', '');
