DROP TABLE agent_tool_reports;

CREATE TABLE world_mail (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    client_message_id   TEXT NOT NULL,
    world_id            TEXT NOT NULL REFERENCES worlds(world_id) ON DELETE CASCADE,
    window_id           TEXT NOT NULL,
    created_at_unix_ms  BIGINT NOT NULL,
    message             TEXT NOT NULL,
    UNIQUE (world_id, window_id, client_message_id)
);

CREATE INDEX world_mail_world_id_id ON world_mail(world_id, id);
