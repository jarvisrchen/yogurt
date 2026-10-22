-- Rebuilds `meetings_fts` to also index `enriched_md`, and moves transcript
-- flattening into the triggers. V004 left `transcript_text` to
-- `MeetingRepo::patch`, so any other writer (the enhance UPSERT, the
-- bootstrap repair) blanked a meeting's transcript out of the index.
--
-- `transcript_text` is the `.text` (or older `.transcript`) of each object in
-- the `transcript_json` array, space-joined. Malformed or non-array JSON
-- indexes as ''. The nested CASE keeps `json_type` off invalid JSON, where it
-- would raise.

DROP TRIGGER meetings_ai;
DROP TRIGGER meetings_ad;
DROP TRIGGER meetings_au;
DROP TABLE meetings_fts;

CREATE VIRTUAL TABLE meetings_fts USING fts5(
  title,
  notes_md,
  enriched_md,
  transcript_text,
  tokenize='unicode61 remove_diacritics 2'
);

CREATE VIEW meetings_fts_source AS
SELECT m.rowid AS rowid, m.title, m.notes_md, m.enriched_md,
  COALESCE((
    SELECT group_concat(t, ' ') FROM (
      SELECT COALESCE(json_extract(j.value, '$.text'),
                      json_extract(j.value, '$.transcript')) AS t
      FROM json_each(COALESCE(
        CASE WHEN json_valid(m.transcript_json) THEN
          CASE WHEN json_type(m.transcript_json) = 'array' THEN m.transcript_json END
        END, '[]')) j
      WHERE j.type = 'object'
    ) WHERE t <> ''
  ), '') AS transcript_text
FROM meetings m;

INSERT INTO meetings_fts(rowid, title, notes_md, enriched_md, transcript_text)
SELECT rowid, title, notes_md, enriched_md, transcript_text FROM meetings_fts_source;

CREATE TRIGGER meetings_ai AFTER INSERT ON meetings BEGIN
  INSERT INTO meetings_fts(rowid, title, notes_md, enriched_md, transcript_text)
  SELECT rowid, title, notes_md, enriched_md, transcript_text
  FROM meetings_fts_source WHERE rowid = new.rowid;
END;

CREATE TRIGGER meetings_ad AFTER DELETE ON meetings BEGIN
  DELETE FROM meetings_fts WHERE rowid = old.rowid;
END;

CREATE TRIGGER meetings_au AFTER UPDATE OF title, notes_md, enriched_md, transcript_json
ON meetings BEGIN
  DELETE FROM meetings_fts WHERE rowid = old.rowid;
  INSERT INTO meetings_fts(rowid, title, notes_md, enriched_md, transcript_text)
  SELECT rowid, title, notes_md, enriched_md, transcript_text
  FROM meetings_fts_source WHERE rowid = new.rowid;
END;
