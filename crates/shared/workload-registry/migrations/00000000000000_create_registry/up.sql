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
    gateway_grant_id  TEXT UNIQUE
);
