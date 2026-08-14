# ADR 0022: Run GitHub Actions jobs in ephemeral KVM guests

- Status: Proposed
- Date: 2026-08-14

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

Add an optional `wt-runner` service. Keep development worlds and `wt-server`
unchanged.

`wt-runner` owns one configured GitHub Actions runner scale set, GitHub App
authentication, just-in-time runner configuration, and the one-job lifecycle.
It uses `wt-libvirt` for machines and a capacity registry shared with
`wt-server`. WT implements the GitHub scale-set protocol directly; it does not
embed Actions Runner Controller, Kubernetes, or another runner manager.

For each requested runner, `wt-runner`:

1. Reserves CPU, memory, and disk capacity.
2. Creates a copy-on-write disk from a dedicated runner image.
3. Starts a guest with a unique identity on a dedicated runner network.
4. Starts one official GitHub Actions runner with a short-lived JIT config.
5. Waits for the runner process to finish.
6. Preserves diagnostics, destroys the guest and disk, and releases capacity.

Cleanup runs after success, failure, cancellation, timeout, or loss of the
runner process. On startup, `wt-runner` destroys recorded runner guests and
overlays before accepting work. It never resumes a job or reuses a runner disk.
Failed cleanup keeps its capacity reservation until reconciliation succeeds.

The runner image contains Ubuntu 24.04, the official GitHub Actions runner,
Docker Engine, and QEMU guest support. It contains no checkout, devcontainer,
Byobu session, SSH access, or agent Git gateway grant.

`wt-server-setup` owns the runner image, strict runtime configuration, systemd
service, dedicated libvirt network and firewall policy, log retention, and an
encrypted systemd credential for the GitHub App private key.

The App credential never enters a guest. Each guest receives only its JIT
configuration and credentials supplied by GitHub for its job. Runner guests
share no writable disk, Docker daemon, checkout, or secret with worlds or other
runners.

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
