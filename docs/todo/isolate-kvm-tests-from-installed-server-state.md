# Isolate KVM tests from installed server state

Real-system KVM tests use isolated gateway ports, sockets, state, and writable
image overlays, but they still depend on the installed server configuration in
`/etc/wt` and the installed golden images.

Running `make install-server`, `make nuke`, or another production installation
workflow concurrently with `make e2e-tests` can remove or replace those shared
files after the E2E preflight has passed. An observed run created its first
world and then failed when `wt-test-server` could no longer read
`/etc/wt/capacity.toml`.

Give real-system tests their own installation namespace, including server and
capacity configuration and golden-image inputs, so production installation
work cannot mutate resources used by an active test run. Update ADR 0038 once
the isolation boundary is implemented.
