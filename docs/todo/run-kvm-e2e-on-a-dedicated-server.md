# KVM E2E installation scope

[ADR 0038](../adr/0038-isolate-kvm-runtime-resources.md) defines isolation
for the KVM harness runtime configuration, credentials, sockets, grants,
provider fixtures, and database. Host preparation invokes the production
server installer with `wt-server.kvm-e2e-install.toml`.

That input contains disposable provider credentials. The production installer
writes host-level state under `/etc/wt`, systemd, `/etc/credstore.encrypted`,
`/var/lib/wt`, and the installed-image paths. The harness runtime namespaces do
not create separate installation paths for those files.

Full-flow KVM E2E therefore shares the production installation namespace while
the installer and lifecycle run. Per-run ports, temporary databases, grants,
and overlays remain runtime-scoped values; they do not create a second
installation namespace.
