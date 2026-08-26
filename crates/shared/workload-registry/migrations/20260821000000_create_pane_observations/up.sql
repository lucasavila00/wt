CREATE TABLE pane_observations (
    world_id TEXT NOT NULL REFERENCES worlds(world_id) ON DELETE CASCADE,
    tmux_session TEXT NOT NULL CHECK (tmux_session = 'wt-host'),
    pane_id TEXT NOT NULL CHECK (pane_id GLOB '%[0-9]*'),
    screen_fingerprint TEXT NOT NULL CHECK (length(screen_fingerprint) = 64),
    changed_at_unix_ms BIGINT NOT NULL,
    observed_at_unix_ms BIGINT NOT NULL,
    PRIMARY KEY (world_id, tmux_session, pane_id)
);

CREATE INDEX pane_observations_world_changed_at
    ON pane_observations(world_id, changed_at_unix_ms DESC);
