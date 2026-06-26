-- yogurt-db schema V004 (Phase 7, Plan 07-02 — FTS5 keyword search).
--
-- Adds a contentless-external FTS5 virtual table indexing every meeting's
-- title, notes_md, and a flattened `transcript_text` projection of the
-- `transcript_json` blob. Three triggers keep the index in sync with the
-- base `meetings` table; explicit maintenance of `transcript_text` happens
-- in `MeetingRepo::patch` because the AFTER-INSERT/UPDATE triggers can only
-- write '' for that column (they don't have the JSON parser handy).
--
-- Ranking: queries use `bm25(meetings_fts)` in the ORDER BY so the most
-- relevant meeting comes first. Tokenizer: `unicode61 remove_diacritics 2`
-- so "café" and "cafe" match interchangeably.
--
-- Storage choice: this is NOT a contentless / external-content FTS5
-- table — it stores its own copy of the indexed columns. The `meetings`
-- base table doesn't have a physical `transcript_text` column (we
-- flatten `transcript_json` in Rust at write time), and an external-
-- content FTS5 table would try to read `transcript_text` off the
-- content table at query time, breaking every MATCH. Self-contained
-- storage costs ~1KB per meeting which is negligible at the scale a
-- local-first app sees (single-digit thousands of meetings, lifetime).
--
-- Cleanup invariant: the `meetings_ad` trigger uses the FTS5 `'delete'`
-- contentless-command form so the index drops the row's rowid cleanly.

CREATE VIRTUAL TABLE meetings_fts USING fts5(
  title,
  notes_md,
  transcript_text,
  tokenize='unicode61 remove_diacritics 2'
);

-- Seed any existing rows (no-op on fresh DB; non-empty on upgrade where
-- Phase 7 Plan 07-01 has already created rows before this migration ran).
-- `transcript_text` seeds as '' here — the row's first PATCH that touches
-- `transcript_json` will replace it with the flattened text.
INSERT INTO meetings_fts(rowid, title, notes_md, transcript_text)
SELECT rowid, title, notes_md, '' FROM meetings;

-- AFTER INSERT — index the new row with an empty transcript_text. The
-- MeetingRepo::create path writes empty transcript_json ("[]") on create,
-- so an empty transcript_text here is correct; patch() maintains the
-- column afterwards.
CREATE TRIGGER meetings_ai AFTER INSERT ON meetings BEGIN
  INSERT INTO meetings_fts(rowid, title, notes_md, transcript_text)
  VALUES (new.rowid, new.title, new.notes_md, '');
END;

-- AFTER DELETE — for a self-contained FTS5 table, a plain DELETE on
-- the virtual table works (the `'delete'` command-form is reserved for
-- contentless tables and would error here).
CREATE TRIGGER meetings_ad AFTER DELETE ON meetings BEGIN
  DELETE FROM meetings_fts WHERE rowid = old.rowid;
END;

-- AFTER UPDATE — drop+re-insert. `transcript_text` is '' here for the same
-- reason as AFTER INSERT; MeetingRepo::patch issues an explicit UPDATE on
-- meetings_fts immediately after to fill it in from the new transcript_json.
CREATE TRIGGER meetings_au AFTER UPDATE ON meetings BEGIN
  DELETE FROM meetings_fts WHERE rowid = old.rowid;
  INSERT INTO meetings_fts(rowid, title, notes_md, transcript_text)
  VALUES (new.rowid, new.title, new.notes_md, '');
END;
