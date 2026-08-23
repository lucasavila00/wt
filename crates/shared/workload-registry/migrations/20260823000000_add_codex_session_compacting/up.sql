ALTER TABLE codex_session_reports
    ADD COLUMN is_compacting BOOLEAN NOT NULL DEFAULT FALSE;
