-- Phase 4: persist the ProseMirror JSON of the enriched doc so TipTap marks
-- (the black/grey distinction) survive restart. The existing enriched_md
-- column keeps wire-format markdown; enriched_doc_json carries the
-- structural ProseMirror representation.
--
-- This is the Phase 4 portion of the STORE-01 split-mapping (CONTEXT D-11).
-- Idempotent against re-application is handled by the migration runner
-- in `storage/migrations.rs` (it inspects PRAGMA table_info before
-- attempting the ALTER on subsequent boots).
ALTER TABLE meetings ADD COLUMN enriched_doc_json TEXT;
