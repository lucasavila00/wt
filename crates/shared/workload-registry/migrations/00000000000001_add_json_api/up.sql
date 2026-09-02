CREATE TABLE server_metadata (
    singleton INTEGER PRIMARY KEY NOT NULL CHECK (singleton = 1),
    server_id TEXT NOT NULL UNIQUE
);

CREATE TABLE api_mutation_results (
    owner TEXT NOT NULL,
    request_id TEXT NOT NULL,
    request_hash TEXT NOT NULL,
    response_json TEXT CHECK (response_json IS NULL OR json_valid(response_json)),
    expires_at_unix_ms BIGINT NOT NULL,
    PRIMARY KEY (owner, request_id)
);

CREATE INDEX api_mutation_results_expiration
    ON api_mutation_results (expires_at_unix_ms);
