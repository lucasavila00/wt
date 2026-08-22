# ADR 0064: Share host SSH authorized keys with worlds

- Status: Accepted
- Date: 2026-08-22

`/home/wt/.ssh/authorized_keys` is the single SSH access policy for the KVM
host and every world. WT atomically publishes it to a dedicated read-only
virtiofs share; a path unit updates running worlds after host changes.

Worlds retain a separate local file only for the temporary provisioning
readiness key. Do not share the host `.ssh` directory: private keys and other
SSH state remain on the host.
