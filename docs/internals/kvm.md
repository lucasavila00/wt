# KVM and NoCloud

`wt-libvirt` owns machine creation, inspection, start, and deletion. It creates
independent qcow2 world disks, libvirt domains, NoCloud seed images, network
identity, and QEMU guest-agent transport.

Every machine seed contains separate files:

- `user-data`: boot-time cloud config;
- `vendor-data`: boot-time vendor config;
- `meta-data`: unique instance and host name;
- `network-config`: DHCP keyed by the unique MAC address.

All three kinds use empty user/vendor cloud config. Retained images create the
shared `wt` login before kind-specific provisioning; host setup stages the
operator recipe through the QEMU guest agent after the network stage and
validates the image-owned UID/GID `1001:1001` contract.

## Images

Retained worlds use two installed images built from the same pinned Ubuntu
source:

- devcontainer: Docker, Git, Dev Container CLI, Byobu, and tmux;
- host: upstream Ubuntu plus OpenSSH, QEMU guest agent, Byobu, tmux, and shared
  terminal assets.

`wt-server-setup` builds each image independently through one KVM builder.
Shared recipes live in `assets/world/shared`; kind recipes live beside their
kind. [ADR 0027](../adr/0027-build-images-in-kvm.md) records the build contract.

Each image has its own provenance manifest and checksum. Image paths cannot be
the same file. A world disk cannot be smaller than its template image; the
provider rejects it before copying and resizing the image. The resulting world
disk has no golden-image backing dependency. Setup may replace stale golden
images automatically without stopping existing worlds.

The shared image foundation is the same in both retained images: user/group
`wt:wt` at UID/GID `1001:1001`, home `/home/wt`, and the shared Byobu/tmux
profile. The image build result is a root-owned `0644` marker with exactly:

```text
kind=KIND
status=ready
recipe_version=1
wt_uid=1001
wt_gid=1001
```

Kind provisioning validates this foundation; it does not create a missing user
or migrate an existing world. Replacing an installed golden image affects only
new worlds. Existing retained disks keep their current guest state and must be
recreated to adopt a changed image foundation.

## Readiness

Libvirt waits for QEMU guest agent and an IPv4 address. Expected failed agent
polls are silent; a timeout names the domain and reports the last libvirt error.
The kind lifecycle then defines readiness:

- devcontainer verifies guest setup and app SSH;
- host starts with SSH disabled, waits for boot cloud-init, creates SSH host
  keys, enables and proves `wt` login, then reports setup, completion, or
  failure markers;
- GitHub CI waits for its runner process and job lifecycle.

Stopped retained guests keep their disk and identity. Missing files, mismatched
identity, or partial libvirt state fail closed.

## Real-system test isolation

The KVM lifecycle test does not mutate installed golden images. Each harness
creates temporary qcow2 overlays backed by those images, applies branch assets
only to the overlays, and keeps them alive until its worlds are deleted.

The production agent Git gateway uses vsock port `18017`. Each harness selects
a different high port and gives the same value to its server, gateway, and
world relays. Test sockets, grants, provider fixtures, and server state stay in
that harness's temporary directory. This lets a KVM run coexist with installed
WT services and with independent test runs without sharing writable runtime
resources.
