# ADR 0008: Allow OSC 52 clipboard writes through Byobu

- Status: Proposed
- Date: 2026-07-17

## Context

[Issue 001](../issues/001-osc52-clipboard-blocked-by-byobu.md) reports that
clipboard writes do not reach the workstation terminal.

WT keeps its persistent Byobu session in the guest and opens each pane in the
devcontainer over SSH. Applications in the devcontainer therefore run inside
the guest's tmux server even though `TMUX` is not present in their environment.
They cannot configure or address that server directly.

Applications use OSC 52 to write to the workstation clipboard. tmux blocks a
raw OSC 52 sequence unless clipboard handling is enabled, and blocks a
tmux-wrapped passthrough sequence unless passthrough is enabled. It also needs
to know that the attached outer terminal supports clipboard writes.

Ghostty advertises itself as `xterm-ghostty`. tmux 3.4 currently covers that
name with its broad built-in `xterm*:clipboard` feature, but relying on that
implicit match would leave WT's supported terminal behavior dependent on
tmux's default compatibility table.

## Decision

Configure the WT-owned tmux server in `/usr/local/share/wt-tmux.conf` with:

```tmux
set-option -s set-clipboard on
set-option -g allow-passthrough on
set-option -as terminal-features ',xterm-ghostty:clipboard'
```

`set-clipboard on` lets tmux accept raw OSC 52 from a pane, store the value in a
tmux buffer, and send the corresponding clipboard sequence to a capable outer
terminal. `allow-passthrough on` also supports applications that wrap OSC 52 in
tmux passthrough DCS.

The `on` passthrough mode limits bypass to visible panes. Do not use `all`,
which would also allow invisible panes to bypass tmux.

Declare only Ghostty's clipboard feature. Do not add a broad terminal-name
wildcard or attempt to describe unrelated Ghostty capabilities in tmux.

Install this configuration during guest provisioning, where WT already creates
the tmux configuration and owns the Byobu backend. Do not add tmux awareness or
terminal-specific handling to the devcontainer or its applications.

The configuration file is read when the WT tmux server starts. New worlds read
it before creating their first session. Updating an existing world requires a
full server restart, such as `byobu kill-server`, rather than detach and
reattach. A restart terminates its existing panes, so it must not be performed
while setup or other work is running.

## Verification

- Verify the provisioned tmux configuration contains the three options with
  their complete values.
- From inside a WT devcontainer attached through `ssh NAME` in Ghostty, emit raw
  OSC 52 containing a known value and verify the local clipboard contains that
  exact value.
- Repeat with OSC 52 wrapped in tmux passthrough DCS.
- Verify passthrough remains limited to a visible pane.
- Restart or recreate the tmux server before an end-to-end check against a
  world that existed before the configuration change.

## Consequences

- Clipboard writes from devcontainer applications work through the normal WT
  Byobu path without application-specific integration.
- A process in the visible pane can change the workstation clipboard. This is
  the intended OSC 52 capability and extends the existing trust placed in code
  running inside a world.
- Raw OSC 52 also updates tmux's buffer because `set-clipboard` is `on` rather
  than `external`.
- Passthrough permits other tmux-wrapped terminal sequences from the visible
  pane, not only OSC 52.
- WT carries one explicit Ghostty compatibility declaration in its tmux
  configuration.
- Applying the change to an existing tmux server interrupts its persistent
  panes.

## Alternatives

### Install Ghostty's complete terminfo entry

Rejected for this fix. We evaluated vendoring a generated terminfo entry and
vendoring Ghostty's pinned Zig capability table with a custom Rust build
generator. The first has weak provenance; the second adds a parser and an
upstream data asset solely to replace one tmux capability declaration.

Generating the entry from a locally installed Ghostty would make WT builds
machine-dependent. Building Ghostty would add a pinned Zig toolchain and
upstream source. Fetching either source or a generated artifact in `build.rs`
would make normal Cargo builds depend on the network and external availability.

Ubuntu 24.04's ncurses packages do not supply `xterm-ghostty`. Full remote
terminal compatibility can be revisited with a distribution or client-owned
terminfo path. It is not required to fix OSC 52 through WT's tmux layer.

### Rely on tmux's built-in `xterm*` feature

Rejected. The current tmux version happens to infer clipboard support, but an
explicit exact-name entry makes WT's Ghostty clipboard contract independent of
that broad upstream default.

### Enable only `set-clipboard`

Rejected. It handles raw OSC 52 but leaves explicitly wrapped passthrough
sequences blocked.

### Enable only passthrough

Rejected. It requires applications to know about the outer tmux layer and does
not make raw OSC 52 work.

### Use `set-clipboard external`

Rejected. It prevents pane applications from using raw OSC 52 to set a tmux
buffer and the outer clipboard.

### Allow passthrough from all panes

Rejected. Invisible panes do not need to bypass tmux to satisfy the clipboard
workflow.

### Configure tmux in the devcontainer

Rejected. The WT-owned tmux server runs in the guest, and the devcontainer
neither contains nor has access to it.
