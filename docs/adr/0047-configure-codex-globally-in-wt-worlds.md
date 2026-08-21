# ADR 0047: Configure Codex in WT worlds

- Status: Proposed
- Date: 2026-08-21

WT will own `/home/wt/.codex/config.toml` in the golden image. It will set
`approval_policy = "never"`, `sandbox_mode = "danger-full-access"`, and a
`SessionStart` hook that runs `wt-tools world-prompt`.

The prompt will explain that the guest is disposable, dependency installation
and system changes are allowed, normal validation should run, and `wt-tools`
is the gateway for provider operations. WT will accept an identical file and
fail on drift rather than merge configuration.
