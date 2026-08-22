# ADR 0064: Share host SSH authorized keys with worlds

- Status: Accepted
- Date: 2026-08-22
- Amends: [ADR 0043](0043-own-retained-guest-foundation-in-shared-images.md)

`/home/wt/.ssh/authorized_keys` is the single SSH access policy for the KVM
host and every world. WT uses the [Codex-auth export pattern](0046-share-codex-auth-with-retained-worlds.md):
it atomically publishes the file to a dedicated read-only virtiofs share; a
path unit republishes changes, so running worlds receive them automatically.

Worlds retain a separate local file only for the temporary provisioning
readiness key. Do not share the host `.ssh` directory: private keys and other
SSH state remain on the host.
