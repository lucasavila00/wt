ALTER TABLE codex_session_reports
    ADD COLUMN pane_generation BIGINT NOT NULL DEFAULT 0;

ALTER TABLE codex_session_reports
    ADD COLUMN pane_sequence BIGINT NOT NULL DEFAULT 0;
