# ADR 0013: Tune managed SSH connections

- Status: Accepted
- Date: 2026-07-31

## Context

WT keeps one workstation SSH connection alive while Byobu owns the persistent
session. The generated SSH config should detect a dead connection, avoid
authentication methods WT does not support, and compress only when useful.

## Decision

Add these options to every managed world alias:

```sshconfig
ServerAliveInterval 30
ServerAliveCountMax 3
PasswordAuthentication no
KbdInteractiveAuthentication no
```

Do not set `BatchMode`; OpenSSH may still prompt to unlock a private key.

Use `Compression yes` for aliases in remote contexts and `Compression no` for
local contexts. The guest-to-container connection stays uncompressed because
it crosses only the local Docker network.

For `NAME-vs`, the main app connection follows the context's compression
setting. Its proxy SSH command always uses `Compression=no` because it carries
an already encrypted app SSH stream.

## Verification

- Snapshot the complete local and remote generated configurations.
- Verify all aliases have the alive and key-only authentication settings.
- Verify compression is off locally and on remotely.
- Verify every `NAME-vs` proxy command overrides compression to `no`.

## Consequences

- A silent dead connection closes after about 90 seconds. Byobu keeps the
  session available for reconnecting.
- Missing keys fail without a useless server-password prompt; private-key
  passphrase prompts still work.
- Remote terminal and VS Code traffic can be compressed without wasting CPU on
  local or already encrypted proxy traffic.
