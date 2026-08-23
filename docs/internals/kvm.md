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

Every image includes current Rust/Cargo, Go, Python/uv, Node.js/nvm, build
tools, CLI utilities including ShellCheck, and Docker/Compose. The finalized
image records the exact resolved tool versions.

The installer first builds a verified, fully sanitized cache image from the
pinned Ubuntu source and development-tools policy. Golden-image rebuilds copy
that cache, refresh WT-specific guest binaries, and publish a standalone image.
The cache identity covers the source image, image size, and development-tools
recipe. WT guest-layer and terminal changes rebuild only the final image. A
cache change picks up newer upstream tool releases. KVM E2E reuses this cache
after its first build in a checkout.

## Readiness

Libvirt waits for the QEMU guest agent and an IPv4 address. The host lifecycle
then creates SSH host keys, applies access state, proves the one-use login, and
removes the readiness key. Failed initial provisioning is not resumed; remove
the world and recreate it from the image.

Stopped worlds keep their disk and identity. Missing files, changed SSH
identity, or partial libvirt state fail closed.

## Full KVM E2E

Full KVM E2E requires a host with the full KVM and libvirt prerequisites. Every
run uses `make clear` with the isolated test Codex sessions path, removes WT
worlds and runtime state, installs the E2E server from the current checkout
into the ordinary host paths, and leaves that marked test server installed. It
never reads or changes active Codex credentials or sessions. The Ubuntu source,
verified golden image, downloads, Cargo artifacts, and build caches remain
available. Image preparation rebuilds only when current provenance does not
match the cache.

The E2E host is a disposable environment with no production workload. Both
runtime cleanup and full server removal are safe there; the normal test path
uses the narrower cleanup so verified images and downloads can be reused.

Individual harnesses use temporary ports, databases, grants, provider fixtures,
and disposable world overlays for deterministic test state. Installation,
image rebuild, reset, and another E2E run cannot execute concurrently.
