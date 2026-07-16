# ADR 0005: Disable remain-on-exit after setup

- Status: Proposed
- Date: 2026-07-16

## Problem statement

WT configures the Byobu tmux server with `remain-on-exit failed`. This is useful
during world setup because a failed installer pane remains available for
inspection while a later connection can open a retry pane.

The option remains enabled after setup completes. A devcontainer pane commonly
exits with a failure status when its shell or SSH connection ends, so tmux keeps
the dead pane. The user must force-kill it instead of exiting normally. This is
especially awkward when restarting a shell to pick up tools installed during
setup.

Setup failures must remain visible, but completed worlds should use normal tmux
pane lifecycle: exiting a shell closes its pane.

## Possible solutions

### Do not enable remain-on-exit

Never set `remain-on-exit`. Normal panes close as expected, but a failed setup
pane also disappears and loses the directly visible failure output. This weakens
the setup recovery behavior.

### Keep remain-on-exit and require users to kill panes

Document the Byobu command for killing a dead pane. This preserves setup output
but makes an implementation detail part of the normal workflow and does not
make exiting or restarting a shell behave as users expect.

### Disable remain-on-exit for individual app panes

Override the option whenever `wt-app-pane` starts. This separates setup and app
behavior at the pane level, but pane option management becomes part of the app
entrypoint and every path that starts an app pane must apply it correctly.

### Disable remain-on-exit when setup completes

Keep `remain-on-exit failed` while the world is in setup, then set it to `off`
for the WT session once the completion marker exists. All later app panes use
normal lifecycle behavior while failed setup panes remain inspectable.

## Preferred strategy

Disable `remain-on-exit` for the `wt-app` tmux session after setup has written
its authoritative completion marker and before the successful installer pane
enters the devcontainer.

Also make `wt-app-shell` reconcile the option from the completion marker on
every attach: keep `remain-on-exit failed` before completion and set it to `off`
after completion. This closes the interruption window between writing the
marker and changing the tmux option, and makes the transition idempotent.

Change the tmux server's global window default, matching the global option WT
sets when it starts the server. This ensures windows created after setup also
inherit `remain-on-exit off`. The setup marker remains the single lifecycle
authority; no additional state is introduced.

This preserves failed installer panes during setup. Once setup succeeds,
exiting a devcontainer shell, including to restart it after tools were
installed, closes the pane without requiring a force kill.
