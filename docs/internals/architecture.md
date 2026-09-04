# Architecture

```text
wt client
  └─ local process or OpenSSH ── wts API bridge
                                  └─ Unix socket (0600)
                                      └─ wts
                                          ├─ registry and capacity
                                          ├─ guest lifecycle
                                          ├─ Git and provider gateway
                                          └─ shared-file publication

wts ── wt-libvirt-kvm ── KVM + QEMU guest agent ── guest
                                                  └─ wtg

standalone Git client
  └─ OpenSSH forced command ── wt-git-proxy ── SSH Git upstream
```

`wts` owns guests. The control plane has no TCP listener. Local
and remote API bridges send one versioned JSON request over stdio to the
protected server socket. The protocol carries world resources, asynchronous
Codex session operations and mailbox entries, a Git author, server information,
server-owned terminal-pane observations, and streamed creation progress events.

## Crates

| Scope | Crates |
|-------|--------|
| WT | `wt-client`, `wt-control-protocol`, `wt-server`, `wt-guest`, `wt-codex-integration`, `wt-server-installer` |
| Agent tool gateway | `wt-agent-tool-gateway`, `wtg tools` |
| Standalone Git proxy | `wt-git-proxy`, `wt-git-proxy-installer` |
| Shared | `wt-libvirt-kvm`, `wt-workload-registry`, `wt-git-smart-protocol`, `wt-installer-support` |
| Tests | `wt-end-to-end-tests` |

Installed WT executables are named by runtime: `wt` is the client, `wts` is
the server runtime, and `wtg` is the guest runtime. Crates retain narrower
internal ownership boundaries and do not each produce an installed program.

`wt-git-smart-protocol` contains the Git protocol bridge and branch write
policy shared by the WT gateway and standalone proxy. `wt-git-proxy` is
released from this workspace but is not part of `wts` or a WT guest.

`wt-installer-support` contains host file, command runner, and SSH credential
handling shared by the regular WT and standalone Git proxy installers.

`wt-guest` owns guest lifecycle and the fixed guest identity. Its server-side
runtime calls image-installed helpers for SSH access, Git author transfer,
agent tooling, and virtiofs Codex session and authentication mounts.

The guest relay polls each Byobu pane whose foreground process is `codex`, then
sends its bounded rendered terminal frame, screen fingerprint, freshness,
current working directory, checked-out Git branch when available, and Byobu
window index and name through its authenticated server connection. `wts` owns
those observations and keeps each world's complete latest snapshot only in
memory. No live pane observation is registry state. No Codex hook or lifecycle
tracker participates in live state.

Each world uses one guest-local Codex App Server daemon. WT runs the native Codex TUI with
`--remote` in a dedicated window of the world's shared Byobu session for each delegated thread.
App Server owns live thread and turn state, and a tmux pane option associates each visible TUI with
its thread. Start, inspect, and send operations use that guest runtime; WT-started turns publish
their terminal result through the durable world mailbox.
After a guest restart, `resume_codex` reopens a persisted thread's missing visible window and
resumes its history without submitting a message. Send remains separate; resume does not create a
replacement thread or restore the interrupted turn.

## Shell playback

`wt shell` owns one SSH/PTY playback connection, reader thread, and `vt100`
parser for each currently SSH-openable world. Reader threads feed one bounded
event queue; the UI loop drains that queue and advances every parser whether a
world is visible or active. Inventory reconciliation can add or remove a world,
and reconnecting replaces its connection, parser, and stream identity.

Pane observations identify the world, tmux session, and pane. A world playback
connection renders the pane currently active in the shared tmux window; it
does not expose every pane in the world. The observation is server-owned and
does not change that shared selection.

The Live control activity renders each preview from the exact observed Codex
pane frame. It does not use a world playback parser, so a non-Codex tab or a
second Codex pane cannot appear in the wrong card. While the Control UI is
open, playback PTYs use the Live preview dimensions; opening a World view
resizes only that world's existing connection to the full viewport. Opening a
preview verifies and selects the observed Codex pane through the existing SSH
control master, then shows that world's shared Byobu session.

The installer builds one development-tools image. It owns current language
toolchains, build and CLI tools, and Docker/Compose with recorded resolved
versions. This is not a runtime world setting.

Provisioning is intentionally restart-only. WT does not resume an interrupted
sequence or repair partial guest state. Remove a failed world and create it
again from the guest image.

## State

- Client contexts: `~/.wt/config.toml`
- Managed SSH: `~/.ssh/wt/`
- Server configuration: `/etc/wt/server.toml`
- Capacity configuration: `/etc/wt/capacity.toml`
- Registry: `~/.local/state/wt/instances.db`
- KVM machine files: the configured libvirt worlds directories

Each registry record is a guest with its resources, backend, disk, and SSH
endpoint. Agent-tool requests are scoped by resolving the accepted vsock peer
CID to a currently active WT libvirt domain and deriving the world UUID from
that domain's name. Parent messages use this world identity and contain no
window or process attribution. Terminal Codex results use the same mailbox and carry a versioned
delivery payload that controllers can correlate with the Codex thread and turn. App Server and
Byobu own the live execution and presentation state.
