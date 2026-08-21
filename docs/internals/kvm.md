# KVM and NoCloud

`wt-libvirt-kvm` owns machine creation, inspection, start, stop, disk usage, and
deletion. It creates
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

Retained worlds use one installed image containing Docker, Git, the Dev
Container CLI, OpenSSH, QEMU guest support, Byobu, tmux, the host setup service,
and shared terminal assets.

`wt-server-installer` builds the image from the pinned Ubuntu source through a
KVM builder. Shared recipes live in `assets/world/shared`; the combined recipe
lives in `assets/world/retained`. [ADR 0027](../adr/0027-build-images-in-kvm.md)
records the build contract, as amended by
[ADR 0049](../adr/0049-use-one-image-for-retained-worlds.md).

The image has one provenance manifest and checksum. A world disk cannot be
smaller than its template image; the provider rejects it before copying and
resizing the image. The resulting world
disk has no golden-image backing dependency. Setup may replace stale golden
images automatically without stopping existing worlds.

The image foundation owns user/group `wt:wt` at UID/GID `1001:1001`, home
`/home/wt`, and the shared Byobu/tmux profile. The image build result is a
root-owned `0644` marker with exactly:

```text
kind=retained
status=ready
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

This restart behavior applies only to an already provisioned world. Initial
retained-world provisioning has no resume path: interruption or failure leaves
a failed world to remove, and retry creates a new disk from the retained image.
WT never continues from partial provisioning state.

## Real-system test isolation

The KVM lifecycle test does not mutate the installed golden image. Each harness
creates a temporary qcow2 overlay backed by that image, applies branch assets
only to the overlay, and keeps it alive until its worlds are deleted.

The production agent tool gateway uses vsock port `18017`. Each harness selects
a different high port and gives the same value to its server, gateway, and
world relays. Test server and capacity configuration, sockets, grants, provider
fixtures, and database state stay in that harness's temporary directory. The
installed golden image is a shared read-only input; the registry cache and
Codex authentication integration are shared host prerequisites. Do not run
installation, image rebuild, or reset workflows while KVM E2E is active.
