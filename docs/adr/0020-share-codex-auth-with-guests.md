# ADR 0020: Make Codex a required guest integration

- Status: Accepted
- Date: 2026-08-21

Every golden image installs Codex and `wt-codex-integration`. The server user
must already be logged in. WT exposes exactly two server-backed resources:

- `/home/wt/.codex/sessions`, mounted read-write in worlds;
- `/home/wt/.codex/.wt-auth`, an atomically updated export containing only
  `auth.json`, mounted read-only in worlds.

The guest links the export under `/home/wt/.codex/auth.json`. A systemd path
unit republishes replacements, so running worlds receive refreshed
authentication without writing it back.

Do not share the complete `.codex` directory. Databases, indexes, logs, locks,
and other runtime state remain local to each world. Shared sessions outlive
worlds and must not be opened concurrently from two worlds.
