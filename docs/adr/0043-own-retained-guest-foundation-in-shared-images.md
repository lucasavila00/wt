# ADR 0043: Own the retained guest foundation in shared images

- Status: Accepted
- Date: 2026-08-20
- Amends: [ADR 0012](0012-separate-image-packages-from-world-configuration.md),
  [ADR 0026](0026-make-world-kinds-first-class.md),
  [ADR 0027](0027-build-images-in-kvm.md),
  [ADR 0039](0039-make-world-disks-independent-of-golden-images.md),
  [ADR 0041](0041-use-protocol-versions-for-client-server-compatibility.md)

## Context

The retained `host` and `devcontainer` images are built independently from the
same pinned Ubuntu source, but both kinds need the same guest owner, home
directory, terminal profile, and Codex mount ownership. Those foundations
must exist before a kind recipe runs. Runtime provisioning must be able to
assume the image contract without silently creating a different guest layout.

The image build also reports success through `/var/lib/wt-image-result`. The
marker has to prove the identity contract that later provisioning and virtiofs
mounts rely on.

## Decision

The shared image recipe creates the `wt` login before the kind recipe runs. It
owns the following contract in both retained images:

- user and primary group `wt` have UID and GID `1001`;
- the login home is `/home/wt` and the shell is `/bin/bash`;
- `/usr/local/share/wt-retained-contract` records the user, UID, GID, and home;
- `/home/wt/.byobu` is owned by `wt:wt` with mode `0755`, and its shared Byobu
  `color` file is owned by `wt:wt` with mode `0644`;
- the shared terminal stack and profile are installed once for both kinds.

The shared terminal configuration is published at
`/usr/local/share/wt-tmux.conf`. The host world uses that profile. The
devcontainer world may layer its application command on top of it and publishes
that kind-specific profile separately; it does not redefine the shared
terminal settings.

The typed `wt-retained-worlds` crate owns the corresponding guest constants and one
provisioning operation for guest access, Git author transfer, agent Git, and
Codex mounts. Both retained kind workers call that complete operation.
The image installs its helpers at
`/usr/local/libexec/wt-retained-access`,
`/usr/local/libexec/wt-retained-git-author`,
`/usr/local/libexec/wt-retained-agent-git`, and
`/usr/local/libexec/wt-retained-mount-codex`.

Git author name and email are common retained-world create fields rather than a
devcontainer-only application detail. They are carried by protocol version 2.
Devcontainer repository setup also records the author in the checkout so the
application container receives it with the repository.

The reusable image leaves guest SSH disabled and without reusable host keys.
Retained provisioning uses the shared access helper to install `wt`'s
authorized keys, generate per-world host keys, and enable guest SSH. App SSH
and its container-specific keys remain devcontainer-owned behavior.

The image build result is root-owned mode `0644` and contains exactly these
four newline-terminated fields, in this order:

```text
kind=KIND
status=ready
wt_uid=1001
wt_gid=1001
```

The image builder validates the complete marker before accepting the build.
Runtime kind provisioning validates the existing image user and fails if the
contract is absent or has drifted; it does not create or repair `wt`.

The server's Codex sessions and every retained VM use this same `1001:1001`
ownership contract. WT mounts sessions at `/home/wt/.codex/sessions` and
injects them into the primary devcontainer.

## Consequences

- Kind recipes can focus on their application stacks and run after a known
  shared foundation.
- A malformed or incompatible image fails during build or provisioning instead
  of receiving kind-specific fallback behavior.
- Replacing a golden image does not rewrite existing world disks. Worlds that
  need a new image-foundation contract must be recreated.
- Changes to shared user or terminal behavior invalidate image provenance and
  require fresh images for newly created worlds.
