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
    created_at_unix_ms BIGINT NOT NULL DEFAULT 0
);

CREATE TABLE agent_tool_reports (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    world_id    TEXT NOT NULL REFERENCES worlds(world_id) ON DELETE CASCADE,
    kind        TEXT NOT NULL CHECK (kind IN ('bug', 'issue', 'improvement', 'feature_request')),
    description TEXT NOT NULL CHECK (length(trim(description)) > 0)
);

CREATE INDEX agent_tool_reports_world_id ON agent_tool_reports(world_id);

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

CREATE TABLE server_metadata (
    singleton INTEGER PRIMARY KEY NOT NULL CHECK (singleton = 1),
    server_id TEXT NOT NULL UNIQUE
);

CREATE TABLE api_mutation_results (
    owner TEXT NOT NULL,
    request_id TEXT NOT NULL,
    request_hash TEXT NOT NULL,
    response_json TEXT CHECK (response_json IS NULL OR json_valid(response_json)),
    expires_at_unix_ms BIGINT NOT NULL,
    preserve_on_restart BOOLEAN NOT NULL DEFAULT 0,
    PRIMARY KEY (owner, request_id)
);

CREATE INDEX api_mutation_results_expiration
    ON api_mutation_results (expires_at_unix_ms);
