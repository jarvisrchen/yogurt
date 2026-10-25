-- sha256 of the meeting bundle as last pushed to Yogurt Cloud. NULL means never pushed.
ALTER TABLE meetings ADD COLUMN cloud_hash TEXT;

-- A pushed meeting deleted locally still needs a deletion marker in the cloud, and the row is gone by then.
-- The trigger leaves a tombstone so the "not pushed" count sees it. A push that sends the marker clears it.
CREATE TABLE cloud_deleted (meeting_id TEXT PRIMARY KEY);
CREATE TRIGGER meetings_cloud_deleted AFTER DELETE ON meetings WHEN old.cloud_hash IS NOT NULL BEGIN
  INSERT OR IGNORE INTO cloud_deleted (meeting_id) VALUES (old.id);
END;
