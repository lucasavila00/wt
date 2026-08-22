# Run KVM E2E on a dedicated server

[ADR 0038](../adr/0038-isolate-kvm-runtime-resources.md) requires the KVM
harness to isolate its runtime configuration, credentials, sockets, grants,
provider fixtures, and database. The harness does that, but preparing its host
still runs the production server installer with
`wt-server.kvm-e2e-install.toml`.

That input contains deliberately fake provider credentials. The production
installer writes them to the real `/etc/wt`, systemd,
`/etc/credstore.encrypted`, `/var/lib/wt`, and installed-image paths. E2E
preparation therefore replaces a usable server before the isolated runtime
harness starts. The two namespaces do not protect the production installation
because both depend on one production installation namespace.

Treat ADR 0038's host-level separation goal as unsuccessful for full-flow KVM
E2E. More namespace conventions are not a credible fix when the test must
exercise the production installer, services, credentials, image publication,
and libvirt lifecycle together.

Simplify the contract: full KVM E2E runs only on a dedicated disposable server
and uses the ordinary production installer and production namespace there. It
is a destructive host workflow. No development or production WT installation
may coexist on that server, and setup may freely nuke, install fixtures, run
the lifecycle, and discard the host afterward.

Follow-up work:

- Amend ADR 0038 and the E2E guide so they do not imply that runtime isolation
  makes E2E preparation safe on a usable WT server.
- Define the dedicated-host lifecycle and make it the only supported KVM E2E
  entrypoint.
- Fail early with a clear warning or explicit dedicated-host acknowledgement
  before installing KVM fixtures.
- Keep per-run ports, temporary databases, grants, and overlays where they make
  tests deterministic, but do not describe them as a second installation
  namespace.
- Remove obsolete dual-namespace machinery once the dedicated-host workflow is
  established.
