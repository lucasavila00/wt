# KVM

`wt-libvirt-kvm` owns machine creation, inspection, start, stop, disk usage,
and deletion. It creates qcow2 world overlays and libvirt domains and uses the
QEMU guest agent for bounded transport and readiness.

Every domain receives a deterministic MAC address derived from its provider
ID. The golden image contains a generic DHCP network configuration keyed by
that MAC. World creation does not attach a configuration seed or run an
operator recipe.

## Images

`wt-server-installer` builds one retained image from the pinned Ubuntu source
inside a temporary KVM guest. The final image contains Git, OpenSSH, QEMU guest
support, Byobu, tmux, Codex, and WT's host and gateway helpers. Build-only
packages and bootstrap state are removed before publication.

The image has a provenance manifest and checksum that cover the static WT guest
binaries. The provider rejects a world disk smaller than its template, then
creates a qcow2 overlay backed by the server's pinned image generation. Image
replacement affects only new worlds; old generations remain available to
existing world overlays.

The image owns `wt:wt` at UID/GID `1001:1001`, `/home/wt`, the shared Byobu/tmux
profile, and the static agent-tool and Codex-integration executables.
Provisioning validates that foundation and applies only world-specific SSH,
Git, gateway, and Codex state.

## Readiness

Libvirt waits for the QEMU guest agent and an IPv4 address. The host lifecycle
then creates SSH host keys, applies access state, proves the one-use login, and
removes the readiness key. Failed initial provisioning is not resumed; remove
the world and recreate it from the image.

Stopped worlds keep their disk and identity. Missing files, changed SSH
identity, or partial libvirt state fail closed.

## Real-system test isolation

The KVM lifecycle test uses a disposable overlay backed by the installed image
and keeps it alive until its worlds are deleted. It uses a unique gateway port
and temporary server, capacity, socket, grant, provider-fixture, and database
state. The installed image and Codex authentication export are shared read-only
prerequisites. Do not install, rebuild images, or reset WT while KVM E2E runs.
