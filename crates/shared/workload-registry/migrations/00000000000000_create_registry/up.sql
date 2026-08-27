CREATE TABLE worlds (
    world_id          TEXT PRIMARY KEY NOT NULL,
    vcpus             BIGINT NOT NULL CHECK (vcpus > 0),
    memory_mib        BIGINT NOT NULL CHECK (memory_mib > 0),
    disk_gib          BIGINT NOT NULL CHECK (disk_gib > 0),
    compute_reserved  BOOLEAN NOT NULL,
    disk_reserved_gib BIGINT NOT NULL CHECK (disk_reserved_gib >= 0),
    owner             TEXT NOT NULL,
    name              TEXT NOT NULL UNIQUE,
    status            TEXT NOT NULL,
    guest_ip          TEXT,
    last_error        TEXT,
    setup_fingerprint TEXT NOT NULL,
    ssh_user          TEXT,
    ssh_host          TEXT,
    ssh_port          INTEGER,
    ssh_host_keys     TEXT NOT NULL,
    gateway_grant_id  TEXT UNIQUE
);

CREATE TABLE pane_observations (
    world_id TEXT NOT NULL REFERENCES worlds(world_id) ON DELETE CASCADE,
    tmux_session TEXT NOT NULL CHECK (tmux_session = 'wt-host'),
    pane_id TEXT NOT NULL CHECK (
        length(pane_id) BETWEEN 2 AND 17
        AND substr(pane_id, 1, 1) = '%'
        AND substr(pane_id, 2) NOT GLOB '*[^0-9]*'
    ),
    screen_fingerprint TEXT NOT NULL CHECK (length(screen_fingerprint) = 64),
    changed_at_unix_ms BIGINT NOT NULL,
    observed_at_unix_ms BIGINT NOT NULL,
    PRIMARY KEY (world_id, tmux_session, pane_id)
);

CREATE INDEX pane_observations_world_changed_at
    ON pane_observations(world_id, changed_at_unix_ms DESC);
