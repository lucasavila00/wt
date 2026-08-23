# ADR 0072: Allow nested KVM E2E runs

- Status: Proposed
- Date: 2026-08-23

## Context

WT development happens inside retained KVM worlds, while the full E2E suite
creates its own KVM guests. Running that suite in a WT world gives contributors
a disposable and reproducible test host, but only when nested KVM is exposed
and the outer world has enough CPU, memory, and disk.

The universal development-tools image build creates an 8-vCPU, 8192-MiB guest.
A two-vCPU, 4-GiB WT world cannot run that build reliably. It is also a poor
place for the cold Rust and image builds: an attempted run spent 6 minutes 37
seconds compiling and bootstrapping before it reached KVM.

WT worlds expose Codex authentication through a symlink. The native-installer
bootstrap test previously validated the development server configuration and
therefore rejected that managed symlink before the KVM suite could start. That
validation is unrelated to the test's native-libvirt linkage assertion and is
already performed with the isolated E2E configuration by `make e2e-tests`.

## Decision

Support full KVM E2E inside a suitably sized WT world when the host exposes
nested KVM. Fail before Rust compilation unless the outer host has at least:

- 8 logical CPUs;
- 12 GiB RAM, leaving 4 GiB outside the 8-GiB image-build guest;
- 20 GiB free on the workspace filesystem;
- readable and writable `/dev/kvm`; and
- the libvirt, QEMU, virt-install, and libguestfs command-line tools.

Recommend 8 vCPUs, 16 GiB RAM, and a 64 GiB disk for the WT world. Keep the
checked-in inner-guest configuration unchanged so nested runs exercise the same
image and lifecycle as a dedicated host.

The bootstrap test will prove that the installer links the host libvirt ABI but
will not validate the unrelated development configuration. The E2E target
continues to prepare and validate its isolated test configuration explicitly.

Before treating nested runs as the normal workflow, try sharing build caches
with the WT world. Measure at least Cargo registry/git data, Cargo target
artifacts, downloaded Ubuntu and package artifacts, and the verified
development-tools image cache. Any writable cache must be scoped so concurrent
worlds cannot corrupt each other's entries; the golden image must remain
checksum-verified and standalone. Retain an unshared fallback until the shared
cache is proven correct under interrupted and concurrent runs.

## Validation

Run one cold and one warm `make e2e-tests` in a suitably sized WT world. Record
time by phase, confirm the cold run publishes the development-tools cache, and
confirm the warm run reuses it without changing its checksum or manifest.
Compare the same runs with and without shared build caches before selecting a
default cache transport and ownership model.

## Consequences

Undersized worlds fail quickly with an actionable resource report instead of
spending minutes compiling first. Nested E2E remains opt-in because it consumes
substantial host resources and depends on nested-virtualization support.

Cache sharing is intentionally unresolved by this proposal. Its measurements
and concurrency behavior determine whether a later implementation can make
nested E2E fast enough for routine use.
