# ADR 0047: Configure Codex in WT worlds

- Status: Proposed; Date: 2026-08-21

## Context

A WT world is disposable and isolated by KVM. Codex should install missing
tooling and run complete validation without repository configuration.

## Decision

WT will own `/home/wt/.codex/config.toml` inside the world. Installation will
create the file when absent, accept it when identical, and fail when it contains
anything else. WT will not merge Codex configuration.

The file will set `approval_policy = "never"` and
`sandbox_mode = "danger-full-access"`. A `SessionStart` hook will run
`wt-tools world-prompt`.

`wt-tools world-prompt` will tell Codex that the guest is disposable, system
changes and dependency installation are allowed, and normal validation should
run. It will also describe `wt-tools` as the gateway for pull requests, reviews,
and CI.

Devcontainer provisioning will install the same file for its `remoteUser`.

## Consequences

Every invocation receives the WT instructions without repository setup.
Unrestricted execution remains inside the disposable guest.
