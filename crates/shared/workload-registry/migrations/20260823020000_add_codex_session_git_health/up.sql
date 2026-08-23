ALTER TABLE codex_session_reports ADD COLUMN git_context_checked_at_unix_ms BIGINT;
ALTER TABLE codex_session_reports ADD COLUMN git_context_error TEXT;
