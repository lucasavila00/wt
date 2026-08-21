# Atomically update Git proxy installation files

The Git proxy installer uses `install source destination` to update live
configuration and SSH credential files. That operation truncates and rewrites
an existing destination while new forced-command SSH processes may be reading
it.

A connection during an update can observe malformed TOML, a partial private
key, or incomplete known hosts data and fail.

Stage each replacement in the destination directory and rename it atomically.
Publish credentials before publishing the configuration that references them.
Test with a reader loop during repeated updates and assert that readers never
observe malformed configuration or partial credentials.

Relevant code:

- `crates/products/git-proxy/installer/src/main.rs`
- `crates/shared/installer-support/src/lib.rs`
- `crates/products/git-proxy/service/src/service.rs`
