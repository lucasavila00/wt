CREATE TABLE codex_session_reports (
    world_id      TEXT NOT NULL REFERENCES worlds(id) ON DELETE CASCADE,
    session_id    TEXT NOT NULL,
    cwd           TEXT NOT NULL CHECK (length(cwd) > 0),
    tmux_session  TEXT NOT NULL CHECK (tmux_session = 'wt-host'),
    pane_id       TEXT NOT NULL CHECK (pane_id GLOB '%[0-9]*'),
    state         TEXT NOT NULL CHECK (state IN ('unknown', 'working', 'needs_attention', 'inactive')),
    received_at_unix_ms BIGINT NOT NULL,
    PRIMARY KEY (world_id, session_id)
);

CREATE INDEX codex_session_reports_session_id_received_at
    ON codex_session_reports(session_id, received_at_unix_ms DESC);
