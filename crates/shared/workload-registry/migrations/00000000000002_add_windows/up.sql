CREATE TABLE windows (
    window_id TEXT PRIMARY KEY NOT NULL,
    world_id TEXT NOT NULL REFERENCES worlds(world_id) ON DELETE CASCADE,
    owner TEXT NOT NULL,
    tmux_window_id TEXT,
    control_token TEXT NOT NULL,
    control_token_hash TEXT NOT NULL,
    argv_json TEXT NOT NULL CHECK (json_valid(argv_json)),
    cwd TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('starting', 'running', 'exited', 'stopped')),
    exit_code INTEGER,
    exit_signal INTEGER,
    next_output_record_id BIGINT NOT NULL DEFAULT 1,
    oldest_available BIGINT NOT NULL DEFAULT 1,
    retained_output_bytes BIGINT NOT NULL DEFAULT 0,
    next_input_sequence_id BIGINT NOT NULL DEFAULT 1,
    output_offset BIGINT NOT NULL DEFAULT 0,
    screen TEXT,
    screen_observed_at_unix_ms BIGINT,
    created_at_unix_ms BIGINT NOT NULL,
    UNIQUE (world_id, tmux_window_id)
);

CREATE INDEX windows_world_id ON windows (world_id);

CREATE TABLE window_output (
    window_id TEXT NOT NULL REFERENCES windows(window_id) ON DELETE CASCADE,
    record_id BIGINT NOT NULL,
    channel TEXT NOT NULL CHECK (channel IN ('stdout', 'stderr')),
    data BLOB NOT NULL,
    PRIMARY KEY (window_id, record_id)
);

CREATE TABLE window_input (
    window_id TEXT NOT NULL REFERENCES windows(window_id) ON DELETE CASCADE,
    sequence_id BIGINT NOT NULL,
    request_id TEXT NOT NULL,
    data BLOB NOT NULL,
    PRIMARY KEY (window_id, sequence_id),
    UNIQUE (window_id, request_id)
);
