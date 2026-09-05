# ADR 0020: Share scoped agent credentials and history

- Status: Accepted
- Date: 2026-08-21

Controllers install and update their agent binaries independently. The server user
must already be logged in. WT exposes exactly two server-backed resources:

- one `/home/wt/.codex/sessions/<world-id>` directory, mounted read-write in
  its matching world;
- `/home/wt/.codex/.wt-auth`, an atomically updated export containing only
  `auth.json`, mounted read-only in worlds.

The guest links the export under `/home/wt/.codex/auth.json`. The `wts` file
watcher republishes replacements, so running worlds receive refreshed
authentication without writing it back.

Do not share the complete `.codex` directory. Databases, indexes, logs, locks,
and other runtime state remain local to each world. Per-world sessions outlive
stops and must not be mounted into another world. See
[ADR 0082](0082-scope-codex-sessions-to-worlds.md) for session isolation.
