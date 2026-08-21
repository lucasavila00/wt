CREATE TABLE disks (
    id TEXT PRIMARY KEY NOT NULL
);

CREATE TABLE guests (
    id           TEXT PRIMARY KEY NOT NULL,
    backend_id   TEXT NOT NULL UNIQUE,
    disk_id      TEXT NOT NULL UNIQUE REFERENCES disks(id),
    vcpus        BIGINT NOT NULL CHECK (vcpus > 0),
    memory_mib   BIGINT NOT NULL CHECK (memory_mib > 0),
    disk_gib     BIGINT NOT NULL CHECK (disk_gib > 0),
    compute_reserved BOOLEAN NOT NULL,
    disk_reserved_gib BIGINT NOT NULL CHECK (disk_reserved_gib >= 0)
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
    gateway_grant_id  TEXT UNIQUE,
    UNIQUE (owner, name)
);
