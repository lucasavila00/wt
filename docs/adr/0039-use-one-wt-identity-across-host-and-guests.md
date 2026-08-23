# ADR 0039: Use one WT identity across host and guests

- Status: Accepted
- Date: 2026-08-22

## Context

Virtiofs passthrough preserves numeric ownership. Guest Codex creates `0600`
rollouts, so equal account names are insufficient when the host and guest
numeric IDs differ. WT therefore needs one fixed identity across that boundary.

## Decision

Every WT host and retained guest has user `wt`, primary group `wt`, UID/GID
`1001:1001`, and home `/home/wt`. This numeric identity is part of the
host/guest filesystem ABI.

`bootstrap-server-user` must reserve those exact IDs and fail before mutation
if either name or number conflicts. It must never choose another ID. Host WT
services run as that account, and host installation must reject a different
effective UID/GID before installing or starting services.

The golden image keeps its existing validated `wt:wt` identity. `/home/wt`,
`/home/wt/.codex`, and the Codex sessions tree are owned by `1001:1001`; the
sessions root is mode `0700`, and Codex rollout files remain mode `0600`.
Virtiofs remains an unmapped passthrough mount. ACL repair,
supplementary-group access, and recursive ownership repair are not part of the
contract.

The libvirt worlds directory is deliberately outside the shared identity
contract. It remains owned by UID `1001` and the host's numeric `kvm` group,
with mode `2770` and search access for `libvirt-qemu`. Server startup and every
domain creation validate that boundary. Tests must reject replacing its host
group and QEMU ACL with ordinary `wt:wt` ownership.

All worlds sharing the sessions tree are therefore the same filesystem
principal and may read or modify it. The tree must contain no host secrets or
control state, and the server must treat its contents as guest-controlled
input.

Enforce the contract at every boundary:

- bootstrap validates the account before creating or changing files;
- server installation validates its effective IDs and all managed paths;
- image publication validates the guest IDs and performs a host/guest mount
  probe with a `0600` file;
- server startup validates its effective IDs and every shared root;
- world creation validates the pinned image identity before creating a domain.

Each failure reports the expected and actual numeric IDs. The clean upgrade
path is to remove all worlds and reinstall the host account, server, image,
and worlds. There is no in-place UID migration or compatibility mode.

## Rejected alternatives

- `wt2` at `1337:1337`: renames and renumbers the same shared principal with no
  stronger invariant.
- Dynamic host IDs plus virtiofs ID mapping: adds a second mapping contract when
  clean-slate hosts can use the canonical identity directly.
- ACL or group repair: guest-created `0600` files can remove effective access.
- A session broker: adds a protocol and service lifecycle that the shared,
  mutually trusted sessions tree does not require.

This decision restores access to rollout files. Selecting the actual first
user prompt instead of injected initialization text is a separate parser fix.
