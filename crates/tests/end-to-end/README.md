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

Run the complete lifecycle on a configured Ubuntu/KVM host:

```text
make e2e-tests
```

Each run uses a unique gateway port, temporary server and capacity
configuration, a disposable overlay on the installed golden image, local Git
and provider fixtures, and an isolated database. The installed image and Codex
authentication export are read-only host prerequisites.

To verify an image prepared from another install input:

```text
make e2e-tests KVM_INSTALL_CONFIG=/path/to/install-input.toml
```

## Clean KVM test host

The test never uses real provider credentials. Create nonfunctional files only
to satisfy the test install input:

```bash
fixture_dir=$HOME/.config/wt/kvm-test
install -d -m 0700 "$fixture_dir"
if [ ! -e "$fixture_dir/id_ed25519" ]; then
    ssh-keygen -q -t ed25519 -N '' -f "$fixture_dir/id_ed25519"
fi
printf 'not-a-real-token\n' > "$fixture_dir/github.token"
awk '{ print "github.com " $1 " " $2 }' \
    "$fixture_dir/id_ed25519.pub" > "$fixture_dir/known_hosts"
chmod 0600 "$fixture_dir/github.token" "$fixture_dir/known_hosts"
```

Install and test:

```bash
scripts/install-server \
    --config examples/server-config/wt-server.kvm-e2e-install.toml
make e2e-tests
```

The serialized KVM flow creates one host world and verifies the slim golden
image, Byobu, scoped Git traffic, agent tools, Codex session/auth sharing,
stop/start persistence, gateway restart, grant revocation, and cleanup.

Do not install, rebuild images, run `make clear`, or run `make nuke` while KVM
E2E is active. Use `make nuke` before installation when a complete reset is
required.
