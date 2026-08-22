ALTER TABLE worlds ADD COLUMN created_at_unix_ms BIGINT NOT NULL DEFAULT 0;

-- Preserve the insertion order of worlds created before this column existed.
UPDATE worlds SET created_at_unix_ms = rowid;
