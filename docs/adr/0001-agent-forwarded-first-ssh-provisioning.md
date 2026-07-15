# ADR 0001: Agent-forwarded, first-SSH world provisioning

- Status: Accepted
- Date: 2026-07-14
- Agent lifetime: Superseded by ADR 0003

## Context

WT originally completed world provisioning as a background server job. The
workstation sent the passphrase for a server-held Git private key, the server
copied temporary Git credentials into the guest, and `wt logs` replayed output
stored in SQLite until the world became usable.

That design made the server responsible for Git credentials, durable job
launching, and provisioning-log storage. It also delayed interactive access to
the guest even though SSH and a persistent terminal session were available
earlier.

WT has not shipped yet. Wire, configuration, and SQLite compatibility are not
required; existing installations can be cleared and reinstalled.

## Decision

WT provisions a world in two stages.

### Synchronous guest preparation

`wt new` synchronously creates and verifies the VM, guest SSH identity, Git,
OpenSSH, the `wt` user and workspace, and Byobu with its tmux backend. It stages
the non-secret installation inputs and returns the world in the `setup` state.

SQLite stores lifecycle and an exact setup-input fingerprint, but no
provisioning output.
Repeated creation of the same `provisioning` or `setup` world is idempotent only
when its source, branch or ref, and Git author match. A retry waits for an
already-running synchronous preparation operation. Different inputs remain a
conflict.

The protocol and registry schema remain version 1. Their version-1 definitions
are replaced in place; no migration or compatibility path is provided.

### First-SSH installation

SSH inventory exposes the following aliases while a world is in `setup`:

- `NAME-host` connects directly to the guest for recovery and does not forward
  an agent or force a command.
- `NAME` pins the guest identity, requests a TTY, enables `ForwardAgent`, and
  runs the WT session entrypoint.
- `NAME-vs` is omitted until setup completes.

The first `ssh NAME` starts the installer in a named Byobu session. All later
connections attach to that session. The entrypoint refreshes Byobu's
`SSH_AUTH_SOCK`; installer attempts are serialized with a guest-held file lock.
A failed pane remains visible, and a later connection creates a retry pane with
the newly forwarded agent socket.

The installer:

1. Requires a forwarded agent for the clone and uses strict host-key checking
   with the staged Git known-hosts file.
2. Reuses a valid matching checkout or removes an incomplete checkout before
   cloning again.
3. Applies the requested checkout and local Git author configuration.
4. Immediately removes clone inputs and the agent from both its environment and
   Byobu's environment.
5. Uses a narrowly sudo-authorized root helper to finish package, Docker,
   registry, Dev Container CLI, and app-SSH preparation.
6. Starts and verifies the devcontainer and its SSH service.
7. Tees output to the attached pane and `/var/lib/wt-setup/install.log`.
8. Removes setup material and temporary sudo access before atomically writing
   the completion marker.
9. Replaces the successful installer process with `wt-app-pane`.

If the workstation disconnects during the clone, the agent socket becomes
unusable and that attempt fails; the next `ssh NAME` retries with a fresh
socket. After the clone, installation no longer depends on the agent and Byobu
keeps it running across disconnects.

### Reconciliation

`get`, `list`, and therefore `wt sync` inspect `setup` and `running` worlds. A
verified completion marker and app SSH identity promote a world to `running`.
Missing domains or changed guest or app identities transition it to `error`.

Running inventory removes agent forwarding, retains `NAME` and `NAME-host`, and
adds the existing `NAME-vs` app alias.

Byobu is the only supported persistent session frontend. The configurable
tmux/Byobu frontend and the Rust `wt-app-shell` binary are removed; the guest
entrypoint is a shell script.

## Consequences

### Benefits

- WT servers no longer hold Git private keys or receive key passphrases.
- Git authentication follows the workstation's normal OpenSSH agent selection.
- The server no longer needs durable provisioning launchers or SQLite log
  chunks.
- `wt new` returns as soon as the recoverable guest SSH endpoint is ready.
- Installation progress and failures are directly visible and resumable in
  Byobu.
- Agent forwarding is limited to setup aliases and removed immediately after
  clone.

### Costs and risks

- Initial setup requires a running workstation SSH agent containing an identity
  accepted by the Git host.
- Anyone controlling the trusted guest or repository during the clone can use
  the forwarded agent for the lifetime of that clone connection. WT does not
  copy or persist the socket.
- A setup world is not editor-ready until a user makes the first `ssh NAME`
  connection and reconciliation observes completion.
- Setup logs live only in the guest. `wt logs` is removed.
- Existing WT installations and databases must be cleared and reinstalled.
- Byobu and its tmux backend are required; selecting plain tmux is no longer
  supported.

## Alternatives considered

### Keep the server-held encrypted Git key

Rejected because it retains passphrase prompting, credential transport, and
server-side key custody solely for the initial clone.

### Forward the workstation agent through the control-plane request

Rejected because the create request ends before later guest-agent operations,
and a Unix agent socket cannot be copied into the VM. Maintaining a relay would
add another credential-bearing subsystem.

### Clone on the server and transfer the checkout to the guest

Rejected because it adds archive or filesystem transfer, changes Git checkout
semantics, and makes the server part of the repository data path.

### Keep background jobs and SQLite logs

Rejected because Byobu already supplies durable execution and an interactive
place to observe output once guest SSH is ready.

### Preserve both tmux and Byobu frontends

Rejected because one Byobu entrypoint is sufficient and removes configuration,
branching behavior, and the Rust session-launcher binary.
