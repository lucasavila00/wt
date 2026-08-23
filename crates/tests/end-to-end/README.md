# wt-end-to-end-tests

Cross-crate tests. This package contains no production code.

| Test | Backend |
|------|---------|
| Service behavior | Injected `WorldWorker` |
| Standalone Git proxy | Real Git and two local OpenSSH hops |
| Full lifecycle | Production `wt-libvirt-kvm` and local KVM |

The workspace suite skips the ignored real-system lifecycle test:

```text
cargo test --workspace
```

Run the complete lifecycle on a full Ubuntu/KVM WT server:

```text
make e2e-tests
```

The E2E environment is disposable and contains no production workload. Runtime
cleanup and a full `make nuke` are both safe on that host.

Every run validates the E2E install input, runs `make clear`, and installs a
test server from the current checkout into the ordinary host paths. Existing
`wt-*` guests, world disks, runtime configuration, grants, the registry, and
generated SSH inventory are removed. The Ubuntu source image, verified golden
image, downloads, Cargo artifacts, and build caches remain in place.

The host is expected to already have the full KVM and libvirt prerequisites.
The installer verifies the cached golden image against current inputs and
rebuilds it only when it is missing or stale. The KVM harness builds the current
checkout's test binaries and installs current guest assets into disposable
overlays.

The test server remains installed after the tests. Local `wt` commands identify
it, `wt shell` displays a test-server badge, and remote OpenSSH WT clients are
rejected.

Each harness uses a unique gateway port, temporary server and capacity
configuration, a disposable overlay on the installed golden image, local Git
and provider fixtures, and an isolated database. These values make individual
runs deterministic; they are not a second installation namespace.

To verify an image prepared from another install input:

```text
make e2e-tests KVM_INSTALL_CONFIG=/path/to/install-input.toml
```

## KVM test host

`make e2e-tests` creates disposable provider credentials and a placeholder
Codex fixture under `~/.config/wt/kvm-test`. `test_server = true` never reads
the active `~/.codex/auth.json` or sessions.

Run the destructive installed-server flow:

```bash
make e2e-tests
```

The KVM suite creates worlds through the API and through `wt shell`. It verifies
the golden image development tools, Byobu, scoped Git traffic, agent tools, Codex
session/auth sharing, stop/start persistence, gateway restart, grant
revocation, and cleanup.

Installation, image rebuild, `make clear`, and `make nuke` cannot run
concurrently with KVM E2E.
