# Libvirt/KVM backend

Production world backend in [`wt-libvirt`](../../crates/wt-libvirt/). Parent: [arch README](./README.md).

## World

| Piece | Choice |
|-------|--------|
| Isolation | KVM guest per instance |
| Image | Ubuntu 24.04 golden image |
| Runtime | Docker Engine + Docker Compose v2 |
| Readiness | QEMU guest agent through libvirt |
| Network | libvirt network; guest IP reported |
| Trust | Local trusted workstation |

KVM is required. No CPU-emulation backend.

## Provision

```text
1. Validate prepared image and /dev/kvm
2. Create qcow2 overlay
3. Create per-world cloud-init identity
4. Define + start KVM domain through libvirt
5. Wait for QEMU guest agent
6. Run docker info + docker compose version through guest agent
7. Era 1.5: clone source + checkout ref
8. Era 1.5: discover one root Compose file + up --build --wait
9. Read guest IP
10. Running
```

## Destroy

```text
1. Destroy active domain
2. Undefine domain + NVRAM
3. Remove world files
```

## Image

`scripts/prepare-image` bakes for Era 1:

- `docker.io`
- `docker-compose-v2`
- `qemu-guest-agent`

World creation does not install packages.

Era 1.5 adds `git` and one pinned small container image used by the offline KVM acceptance test. The manifest records both.

## One-line summary

**Prepared Ubuntu image → KVM guest → clone + checkout → Compose ready.**
