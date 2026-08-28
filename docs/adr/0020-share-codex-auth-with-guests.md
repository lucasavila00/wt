# ADR 0020: Make Codex a required guest integration

- Status: Amended by [ADR 0082](0082-scope-codex-sessions-to-worlds.md)
- Date: 2026-08-21

Every golden image installs Codex and `wt-codex-integration`. The server user
must already be logged in. WT exposes exactly two server-backed resources:

- one `/home/wt/.codex/sessions/<world-id>` directory, mounted read-write in
  its matching world;
- `/home/wt/.codex/.wt-auth`, an atomically updated export containing only
  `auth.json`, mounted read-only in worlds.

The guest links the export under `/home/wt/.codex/auth.json`. A systemd path
unit republishes replacements, so running worlds receive refreshed
authentication without writing it back.

Do not share the complete `.codex` directory. Databases, indexes, logs, locks,
and other runtime state remain local to each world. Per-world sessions outlive
stops and must not be mounted into another world.
