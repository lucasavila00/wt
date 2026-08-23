# ADR 0021: Configure Codex in WT worlds

- Status: Accepted
- Date: 2026-08-21

WT will own `/home/wt/.codex/config.toml` in the golden image. It will set
`approval_policy = "never"`, `sandbox_mode = "danger-full-access"`, and a
`SessionStart` hook that runs `wt-tools world-prompt`.

The golden image will also own `/etc/codex/requirements.toml`. Its
`models.new_thread` settings provide the default model for new Codex threads:
`gpt-5.6-terra` with `high` reasoning effort. Codex treats these as managed
defaults, not enforcement, so an explicit model or reasoning selection can
override them without modifying the user configuration.

The prompt will explain that the guest is disposable, dependency installation
and system changes are allowed, normal validation should run, and `wt-tools`
is the gateway for provider operations. WT will accept an identical file and
fail on drift rather than merge configuration.
