CREATE TABLE hosts (
    id               TEXT PRIMARY KEY NOT NULL REFERENCES worlds(id) ON DELETE CASCADE,
    gateway_grant_id TEXT UNIQUE
);

INSERT INTO hosts (id)
SELECT worlds.id
FROM worlds
JOIN guests ON guests.id = worlds.id
WHERE guests.kind = 'host';
