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
    gateway_grant_id  TEXT UNIQUE,
    created_at_unix_ms BIGINT NOT NULL DEFAULT 0
);

CREATE TABLE agent_tool_reports (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    world_id    TEXT NOT NULL REFERENCES worlds(world_id) ON DELETE CASCADE,
    kind        TEXT NOT NULL CHECK (kind IN ('bug', 'issue', 'improvement', 'feature_request')),
    description TEXT NOT NULL CHECK (length(trim(description)) > 0)
);

CREATE INDEX agent_tool_reports_world_id ON agent_tool_reports(world_id);

CREATE TABLE pane_observations (
    world_id TEXT NOT NULL REFERENCES worlds(world_id) ON DELETE CASCADE,
    tmux_session TEXT NOT NULL CHECK (tmux_session = 'wt-host'),
    pane_id TEXT NOT NULL CHECK (
        length(pane_id) BETWEEN 2 AND 17
        AND substr(pane_id, 1, 1) = '%'
        AND substr(pane_id, 2) NOT GLOB '*[^0-9]*'
    ),
    screen_fingerprint TEXT NOT NULL CHECK (length(screen_fingerprint) = 64),
    cwd TEXT NOT NULL CHECK (length(cwd) BETWEEN 1 AND 4096),
    git_branch TEXT CHECK (length(git_branch) BETWEEN 1 AND 255),
    changed_at_unix_ms BIGINT NOT NULL,
    observed_at_unix_ms BIGINT NOT NULL,
    PRIMARY KEY (world_id, tmux_session, pane_id)
);

CREATE INDEX pane_observations_world_changed_at
    ON pane_observations(world_id, changed_at_unix_ms DESC);

CREATE TABLE repositories (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    provider_host TEXT NOT NULL,
    repository TEXT NOT NULL,
    UNIQUE (provider_host, repository)
);

CREATE TABLE world_git_activity (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    world_id TEXT NOT NULL REFERENCES worlds(world_id) ON DELETE CASCADE,
    recorded_at_unix_ms BIGINT NOT NULL,
    kind TEXT NOT NULL,
    repository_id INTEGER NOT NULL REFERENCES repositories(id),
    git_service TEXT,
    branch TEXT,
    previous_oid TEXT,
    new_oid TEXT
);

CREATE INDEX world_git_activity_world_id_id
    ON world_git_activity (world_id, id DESC);
CREATE INDEX world_git_activity_target_branch_id
    ON world_git_activity (repository_id, branch, id DESC);

CREATE TABLE world_wt_tools_activity (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    world_id TEXT NOT NULL REFERENCES worlds(world_id) ON DELETE CASCADE,
    recorded_at_unix_ms BIGINT NOT NULL,
    repository_id INTEGER NOT NULL REFERENCES repositories(id),
    action TEXT NOT NULL,
    branch TEXT,
    change_request TEXT,
    request_json TEXT NOT NULL CHECK (json_valid(request_json)),
    response_json TEXT NOT NULL CHECK (json_valid(response_json))
);

CREATE INDEX world_wt_tools_activity_world_id_id
    ON world_wt_tools_activity (world_id, id DESC);
CREATE INDEX world_wt_tools_activity_target_branch_id
    ON world_wt_tools_activity (repository_id, branch, id DESC);
CREATE INDEX world_wt_tools_activity_target_change_request_id
    ON world_wt_tools_activity (repository_id, change_request, id DESC);
