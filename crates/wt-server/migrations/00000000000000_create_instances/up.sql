CREATE TABLE disk_nodes (
    id        TEXT PRIMARY KEY NOT NULL,
    parent_id TEXT REFERENCES disk_nodes(id),
    immutable BOOLEAN NOT NULL
);

CREATE TABLE instances (
    id                TEXT PRIMARY KEY NOT NULL,
    owner             TEXT NOT NULL,
    name              TEXT NOT NULL,
    status            TEXT NOT NULL,
    guest_ip          TEXT,
    last_error        TEXT,
    backend_id        TEXT NOT NULL UNIQUE,
    head_disk_id      TEXT NOT NULL UNIQUE REFERENCES disk_nodes(id),
    source            TEXT NOT NULL,
    vcpus             BIGINT NOT NULL,
    memory_mib        BIGINT NOT NULL,
    disk_gib          BIGINT NOT NULL,
    setup_fingerprint TEXT NOT NULL,
    ssh_user          TEXT,
    ssh_host          TEXT,
    ssh_port          INTEGER,
    ssh_host_keys     TEXT NOT NULL,
    app_ssh_user      TEXT,
    app_ssh_port      INTEGER,
    app_ssh_host_keys TEXT NOT NULL,
    UNIQUE (owner, name)
);
