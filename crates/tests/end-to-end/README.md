# wt-end-to-end-tests

Cross-crate tests. This package contains no production code.

| Test | Backend |
|------|---------|
| Service behavior | Injected `WorldWorker` |
| Standalone Git proxy | Real Git and two local OpenSSH hops |
| Full lifecycle | Production `wt-libvirt-kvm` and local KVM |

Tests use the production API, reservation, background job, lock, registry, log,
and recovery paths. The KVM test uses the installed devcontainer and host
images and registry cache.

Run from the workspace root:

```text
cargo test --workspace
```

The workspace command skips the ignored full-lifecycle test. Each full-lifecycle
run uses a unique gateway port and disposable overlays on the installed golden
images, so it can run beside installed WT services and other test runs. Run it
on a configured Ubuntu/KVM host with:

```text
make e2e-tests
```

The target first checks the installed Codex authentication and registry-cache
prerequisites, then runs the read-only image provenance check. It does not read
installed server or capacity configuration. To verify images prepared from a
different install input, override `KVM_INSTALL_CONFIG`:

```text
make e2e-tests KVM_INSTALL_CONFIG=/path/to/install-input.toml
```

Host setup: [Development](../../DEVELOPMENT.md).

## Clean KVM test host

The E2E test uses a local bare Git repository and an in-process provider API
fixture. It must not use real Git provider credentials. On a dedicated test
host, create nonfunctional provider files only to satisfy the host install
input:

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

Prepare the host with the test-only install input. The test starts its own
server and agent tool gateway with temporary server and capacity configuration.
It uses the installed golden images as read-only inputs and the installed
registry cache and Codex authentication integration as host prerequisites:

```bash
scripts/install-server \
    --config examples/server-config/wt-server.kvm-e2e-install.toml
make e2e-tests
```

After `make clear`, the prerequisites remain installed and `make e2e-tests`
can run without reinstalling or rebuilding images. Do not run installation,
image rebuild, `make clear`, or `make nuke` while KVM E2E is active.

The serialized KVM flow creates devcontainer and host worlds together. It runs
the checked-in host recipe in Byobu with a disposable forwarded agent, checks
the staged bytes, restart persistence, app-container recovery, fake-provider
Git traffic, and cleanup. A second recipe fails cloud-init and must remain
accessible in `error` until the test deletes it.

To discard an existing installation first, run `make nuke`. This is destructive:
it removes every `wt-*` libvirt domain, the world disks and SQLite registry,
installed images, registry cache, installed services, and encrypted credentials.
Re-run the test-only install command afterward. A clean libvirt inventory can
be confirmed with:

```bash
virsh -c qemu:///system list --all
```
