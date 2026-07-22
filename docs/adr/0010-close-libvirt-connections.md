# ADR 0010: Close libvirt connections

- Status: Accepted
- Date: 2026-07-22

## Context

Repeated world operations eventually make `wt-server` unable to connect to
libvirt with `Failed to create socket: Too many open files`. Once this happens,
creation fails and reconciliation marks otherwise running worlds as errors.

`wt-libvirt` opens a new `virt::connect::Connect` for lifecycle and inspection
operations. In `virt` 0.4.3, `Connect` does not implement `Drop`; callers must
call `Connect::close` themselves. WT does not do so, including in the repeated
inspection performed by `wt ls`, and therefore leaks a libvirt socket from the
long-lived server process on each connection.

Raising the service's open-file limit would only delay the same failure. The
normal number of worlds does not require an unusually high descriptor limit.

## Decision

Own every opened libvirt connection through a private RAII type in
`wt-libvirt`. Its destructor calls `Connect::close`, so success, error, and
early-return paths all release WT's connection reference.

Use that owner at every `Connect::open` site. Keep domain and network handles
inside the connection owner's lifetime where practical. A libvirt object may
temporarily retain its own connection reference; closing WT's reference and
then dropping the object must still release the connection.

Do not raise `LimitNOFILE` in `wt-server.service`. Do not add connection-limit
configuration. Keep opening short-lived connections for individual operations
rather than sharing a connection across concurrent server requests.

Restart `wt-server.service` when deploying the fix. Restarting releases
descriptors already leaked by the old process; subsequent operations close
their connections normally.

## Verification

- Repeatedly list and reconcile multiple running worlds and verify the
  `wt-server` process's open-descriptor count remains bounded.
- Exercise successful and failing libvirt operations and verify both release
  their connection reference.
- Verify world creation, inspection, and deletion still work against the real
  libvirt/KVM system.
- Verify the installed service unit does not increase `LimitNOFILE`.

## Consequences

- Long-running servers no longer exhaust file descriptors through libvirt
  inspection or lifecycle operations.
- Recovery from an already exhausted process requires one service restart.
- Connection cleanup is explicit at the wrapper boundary despite the upstream
  Rust type not implementing `Drop`.
- A cleanup failure cannot be returned from a destructor; operational errors
  remain the primary result of the request.

## Alternatives

### Raise the systemd open-file limit

This increases the time before failure but leaves descriptor use unbounded and
can consume more host resources before the server stops working.

### Keep one shared libvirt connection

This would avoid repeated opens, but it adds synchronization and reconnect
policy to concurrent request handling. Deterministic cleanup fixes the leak
without changing the current operation model.

### Call `Connect::close` at each return site

Manual cleanup is easy to miss when a function gains a new error or early
return. One private owner enforces the requirement at every scoped exit.
