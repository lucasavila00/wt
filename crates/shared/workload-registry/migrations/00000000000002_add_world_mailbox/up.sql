CREATE TABLE world_mail (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    world_id            TEXT NOT NULL REFERENCES worlds(world_id) ON DELETE CASCADE,
    created_at_unix_ms  BIGINT NOT NULL,
    message             TEXT NOT NULL
);

CREATE INDEX world_mail_world_id_id ON world_mail(world_id, id);
