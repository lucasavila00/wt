CREATE TABLE disk_nodes (
    id        TEXT PRIMARY KEY NOT NULL,
    parent_id TEXT REFERENCES disk_nodes(id),
    immutable BOOLEAN NOT NULL
);

CREATE TABLE guests (
    id           TEXT PRIMARY KEY NOT NULL,
    kind         TEXT NOT NULL CHECK (kind IN ('devcontainer', 'host', 'github-ci')),
    backend_id   TEXT NOT NULL UNIQUE,
    head_disk_id TEXT NOT NULL UNIQUE REFERENCES disk_nodes(id),
    vcpus        BIGINT NOT NULL CHECK (vcpus > 0),
    memory_mib   BIGINT NOT NULL CHECK (memory_mib > 0),
    disk_gib     BIGINT NOT NULL CHECK (disk_gib > 0)
);

CREATE TABLE worlds (
    id                TEXT PRIMARY KEY NOT NULL REFERENCES guests(id) ON DELETE CASCADE,
    owner             TEXT NOT NULL,
    name              TEXT NOT NULL,
    status            TEXT NOT NULL,
    guest_ip          TEXT,
    last_error        TEXT,
    setup_fingerprint TEXT NOT NULL,
    ssh_user          TEXT,
    ssh_host          TEXT,
    ssh_port          INTEGER,
    ssh_host_keys     TEXT NOT NULL,
    UNIQUE (owner, name)
);

CREATE TABLE devcontainers (
    id                TEXT PRIMARY KEY NOT NULL REFERENCES worlds(id) ON DELETE CASCADE,
    source            TEXT NOT NULL,
    git_base          TEXT NOT NULL,
    git_prefix        TEXT NOT NULL,
    gateway_grant_id  TEXT NOT NULL UNIQUE,
    app_ssh_user      TEXT,
    app_ssh_port      INTEGER,
    app_ssh_host_keys TEXT NOT NULL
);

CREATE TABLE runners (
    id               TEXT PRIMARY KEY NOT NULL REFERENCES guests(id) ON DELETE CASCADE,
    name             TEXT NOT NULL UNIQUE,
    status           TEXT NOT NULL,
    github_runner_id BIGINT UNIQUE,
    last_error       TEXT
);
