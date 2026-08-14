# Worlds

A world is an isolated KVM guest with an immutable kind. Every world has a
registry identity, CPU/RAM/disk reservation, writable disk, network identity,
and cleanup owner.

`devcontainer` and `host` worlds have an owner and name. They remain until
removed and keep their disk and SSH host identity across stops and starts.
`github-ci` worlds are system-named, not user-addressable, and destroyed after
one job.

All three kinds use the same capacity admission and disk registry. The future
CI operator must use the retained server's registry so a host cannot over-admit
resources already reserved by a devcontainer or runner.

Kind-specific behavior is represented and stored separately instead of using
empty fields on one generic record.

See [devcontainer](./devcontainer.md), [host](./host.md), or
[GitHub CI](./github-ci.md).
