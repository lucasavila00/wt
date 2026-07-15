# ADR 0002: Route new Byobu panes to the devcontainer

- Status: Proposed
- Date: 2026-07-15

## Context

Byobu runs in the guest.

WT starts the first pane with `wt-app-pane`. That pane enters the devcontainer.

Byobu starts new windows and splits without a command. tmux uses the guest
shell. Those panes stay in the guest.

All panes opened through `ssh NAME` must enter the devcontainer.

## Decision

Set the WT tmux session's `default-command` to
`/usr/local/bin/wt-app-pane`.

Keep the first pane's explicit command. During setup it runs
`wt-setup-world`. After setup it runs `wt-app-pane`. Explicit commands override
`default-command`.

Use `ssh NAME-host` when a guest shell is needed.

## Verification

- The first pane still runs setup, then enters the devcontainer.
- A new Byobu window enters the devcontainer.
- A new Byobu split enters the devcontainer.
- Reattaching keeps the existing session.

## Consequences

- All normal Byobu panes enter the devcontainer.
- Byobu stays in the guest and survives container restarts.
- If the devcontainer is down, new panes fail through `wt-app-pane`.

## Alternatives

### Change Byobu key bindings

Rejected. It misses panes created by other commands.

### Run Byobu in the devcontainer

Rejected. The session would die with the container.

### Enter from guest shell startup files

Rejected. It would also affect `ssh NAME-host` recovery shells.
