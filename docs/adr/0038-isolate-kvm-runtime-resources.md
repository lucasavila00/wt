# ADR 0038: Isolate KVM test runtime resources

- Status: Accepted
- Date: 2026-08-16

Each KVM harness chooses a unique gateway port and keeps server configuration,
capacity configuration, sockets, grants, provider fixtures, and database state
under its temporary directory. It uses a disposable overlay backed by the
installed image and never modifies that image.

The installed image and Codex authentication export are shared host
prerequisites. Installation, image rebuild, and reset workflows must not run
concurrently with KVM E2E.
