# Worlds

A world is an isolated KVM guest with an immutable kind. Every world has a
registry identity, CPU/RAM/disk reservation, writable disk, network identity,
and cleanup owner.

Shipped `devcontainer` and `host` worlds have an owner and name. They remain
until removed and keep their disk and SSH host identity across stops and
starts.

The GitHub CI foundation models system-named, single-job worlds. Its future
operator will destroy each world after the job; that operator is not shipped.

All three kinds use the same capacity and disk registry model. The future CI
operator must open the same registry so it cannot over-admit the host.

Kind-specific behavior is represented and stored separately instead of using
empty fields on one generic record.

See [devcontainer](./devcontainer.md), [host](./host.md), or
[GitHub CI](./github-ci.md).
