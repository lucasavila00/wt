# ADR 0023: Run GitHub Actions jobs in ephemeral KVM guests

- Status: Proposed
- Date: 2026-08-14
- Amended by: [ADR 0026](0026-make-world-kinds-first-class.md)

## Context

WT already creates isolated KVM guests from a golden image, tracks host
capacity, and destroys guests through libvirt. Those primitives also fit
self-hosted GitHub Actions runners.

A development world is the wrong runner boundary. Worlds are named,
interactive, long-lived, and built from a repository's devcontainer. Runners
must be anonymous, non-interactive, controlled by the operator, and removed
after one job.

GitHub recommends ephemeral runners for autoscaling. Actions Runner Controller
provides that lifecycle for Kubernetes, but WT hosts do not otherwise need
Kubernetes. GitHub publishes a scale-set protocol for other infrastructure
providers.

## Decision

Add an optional `wt-runner` service, separate from retained-world lifecycle and
the `wt` client. The shared `guests`, `worlds`, and `runners` registry foundation
is already present. Installing that schema over an older server requires
`make nuke`; it has no migration.

`wt-runner` will own one configured GitHub Actions runner scale set, GitHub App
authentication, just-in-time runner configuration, and the one-job lifecycle.
It uses `wt-libvirt` for machines and a capacity registry shared with
`wt-server`. WT implements the GitHub scale-set protocol directly; it does not
embed Actions Runner Controller, Kubernetes, or another runner manager.

For each requested runner, `wt-runner` will:

1. Reserve CPU, memory, and disk capacity.
2. Create an independent disk from a dedicated runner image.
3. Start a guest with a unique identity on a dedicated runner network.
4. Start one official GitHub Actions runner with a short-lived JIT config.
5. Wait for the runner process to finish.
6. Preserve diagnostics, destroy the guest and disk, and release capacity.

Cleanup will run after success, failure, cancellation, timeout, or loss of the
runner process. On startup, `wt-runner` will destroy recorded runner guests and
overlays before accepting work. It will never resume a job or reuse a runner
disk. Failed cleanup will keep its reservation until reconciliation succeeds.

The runner image will contain Ubuntu 24.04, the official GitHub Actions runner,
Docker Engine, and QEMU guest support. It will contain no checkout,
devcontainer, Byobu session, SSH access, or agent Git gateway grant.

`wt-server-setup` will own the runner image, strict runtime configuration,
systemd service, dedicated libvirt network and firewall policy, log retention,
and encrypted GitHub App credential.

The App credential will never enter a guest. Each guest will receive only its
JIT configuration and job credentials. Runner guests will share no writable
disk, Docker daemon, checkout, or secret with another guest.

GitHub remains authoritative for workflow status, logs, artifacts, and
cancellation. WT keeps service and runner diagnostic logs, but adds no CI UI or
second job log store.

Implement configuration, validation, capacity admission, lifecycle state, and
reconciliation in Rust. POSIX shell assets may install and start the runner in
the guest. The first version supports GitHub Actions only.

## Consequences

- Every job gets a fresh VM, disk, Docker daemon, and runner registration.
- Worlds keep their current lifecycle and trust model.
- Runners and worlds compete atomically for finite host capacity.
- WT gains a continuous dependency on GitHub's scale-set API and runner
  versions.
- Operators must build a separate image and retain diagnostics outside the VM.

## Alternatives

Existing worlds and persistent runner VMs were rejected because state and
credentials could cross job boundaries.

Actions Runner Controller was rejected because it requires Kubernetes.

External or commercial runner managers were rejected because WT must retain
ownership of capacity, libvirt, credentials, and cleanup.
