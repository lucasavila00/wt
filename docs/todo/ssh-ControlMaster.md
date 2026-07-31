# ADR: Enable SSH connection multiplexing

## Context
Every SSH/SCP/rsync/Git operation creates a new connection, re-authenticates, and re-establishes ProxyJump tunnels.

## Decision
Enable OpenSSH connection multiplexing globally:

ControlMaster auto
ControlPersist 15m
ControlPath ~/.ssh/cm-%C

## Acceptance Criteria
- First connection behaves unchanged.
- Subsequent connections to the same host reuse the existing socket.
- Works transparently with ProxyJump.
- Auto-creates ~/.ssh/ if needed and a control socket directory with 0700 permissions.
- Gracefully falls back if multiplexing is unavailable.
- Document how to inspect (`ssh -O check`) and close (`ssh -O exit`) master connections.
