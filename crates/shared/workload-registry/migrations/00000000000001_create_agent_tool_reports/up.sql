CREATE TABLE agent_tool_reports (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    world_id    TEXT NOT NULL REFERENCES worlds(id) ON DELETE CASCADE,
    kind        TEXT NOT NULL CHECK (kind IN ('bug', 'issue', 'improvement', 'feature_request')),
    description TEXT NOT NULL CHECK (length(trim(description)) > 0)
);

CREATE INDEX agent_tool_reports_world_id ON agent_tool_reports(world_id);
