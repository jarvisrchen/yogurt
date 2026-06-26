-- yogurt-db schema V005 (Phase 8, Plan 08-03 — STT provider + model selection).
--
-- The plan stub called for `ALTER TABLE settings ADD COLUMN stt_model TEXT`,
-- but the Phase 5 `settings` table is a KV store (`key TEXT PRIMARY KEY,
-- value TEXT NOT NULL`) — see V001__initial.sql. There is no schema to
-- ALTER. We seed two new KV rows (idempotent via INSERT OR IGNORE) so
-- meetings/start.rs can branch on `stt.provider` and `stt.model`:
--
--   stt.provider = "cloud" | "local"        — default "cloud" (Deepgram)
--   stt.model    = "tiny.en" | "small.en"   — default "small.en" per
--                | "medium.en" | "large-v3"   D-02 (LOCAL baseline)
--
-- Rust typed projection lives in `yogurt-db::settings::General` with two
-- new fields `stt_provider` + `stt_model`, both defaulting to the seeded
-- values when the key is absent from the table.

INSERT OR IGNORE INTO settings(key, value) VALUES
    ('stt.provider', 'cloud'),
    ('stt.model',    'small.en');
