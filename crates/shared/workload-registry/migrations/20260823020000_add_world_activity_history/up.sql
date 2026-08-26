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
