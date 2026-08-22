# wt-end-to-end-tests

Cross-crate tests. This package contains no production code.

| Test | Backend |
|------|---------|
| Service behavior | Injected `WorldWorker` |
| Standalone Git proxy | Real Git and two local OpenSSH hops |
| Full lifecycle | Production `wt-libvirt-kvm` and local KVM |

The workspace suite skips the ignored real-system lifecycle test:

```text
scripts/cargo test --workspace
```

Run the complete lifecycle on a full Ubuntu/KVM WT server:

```text
make e2e-tests
```

`make e2e-tests` is destructive. It validates the E2E install input, runs
`scripts/nuke`, and installs a test server through the ordinary production
installation paths. This removes the current WT services, credentials, worlds,
images, client configuration, and Codex export before the test install starts.

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

The test install uses disposable provider credentials:

```bash
fixture_dir=$HOME/.config/wt/kvm-test
install -d -m 0700 "$fixture_dir"
if [ ! -e "$fixture_dir/id_ed25519" ]; then
    ssh-keygen -q -t ed25519 -N '' -f "$fixture_dir/id_ed25519"
fi
printf 'not-a-real-token\n' > "$fixture_dir/github.token"
chmod 0600 "$fixture_dir/github.token"
```

Run the destructive installed-server flow:

```bash
make e2e-tests
```

The KVM suite creates worlds through the API and through `wt shell`. It verifies
the slim golden image, Byobu, scoped Git traffic, agent tools, Codex
session/auth sharing, stop/start persistence, gateway restart, grant
revocation, and cleanup.

Installation, image rebuild, `make clear`, and `make nuke` cannot run
concurrently with KVM E2E.
