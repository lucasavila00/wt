CREATE TABLE world_git_activity (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    world_id TEXT NOT NULL REFERENCES worlds(id) ON DELETE CASCADE,
    recorded_at_unix_ms BIGINT NOT NULL,
    kind TEXT NOT NULL,
    provider_host TEXT NOT NULL,
    repository TEXT NOT NULL,
    git_service TEXT,
    branch TEXT,
    previous_oid TEXT,
    new_oid TEXT
);

CREATE INDEX world_git_activity_world_id_id
    ON world_git_activity (world_id, id DESC);
CREATE INDEX world_git_activity_target_branch_id
    ON world_git_activity (provider_host, repository, branch, id DESC);

CREATE TABLE world_wt_tools_activity (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    world_id TEXT NOT NULL REFERENCES worlds(id) ON DELETE CASCADE,
    recorded_at_unix_ms BIGINT NOT NULL,
    provider_host TEXT NOT NULL,
    repository TEXT NOT NULL,
    action TEXT NOT NULL,
    branch TEXT,
    change_request TEXT,
    request_json TEXT NOT NULL CHECK (json_valid(request_json)),
    response_json TEXT NOT NULL CHECK (json_valid(response_json))
);

CREATE INDEX world_wt_tools_activity_world_id_id
    ON world_wt_tools_activity (world_id, id DESC);
CREATE INDEX world_wt_tools_activity_target_branch_id
    ON world_wt_tools_activity (provider_host, repository, branch, id DESC);
CREATE INDEX world_wt_tools_activity_target_change_request_id
    ON world_wt_tools_activity (provider_host, repository, change_request, id DESC);
