# wt-integration-tests

Cross-crate tests. This package contains no production code.

| Test | Backend |
|------|---------|
| Service behavior | Injected `WorldWorker` |
| Full lifecycle | Production `wt-libvirt` and local KVM |

Tests use the production API, reservation, background job, lock, registry, log,
and recovery paths. The KVM test uses the installed devcontainer and host
images and registry cache.

Run from the workspace root:

```text
cargo test --workspace
```

The workspace command skips the ignored full-lifecycle test. Run that test on a
configured Ubuntu/KVM host with:

```text
make e2e-tests
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

Prepare the host with the test-only install input, then stop the installed
services. The test starts its own server and agent Git gateway, while retaining
the installed images and registry cache:

```bash
scripts/install-server \
    --config examples/server-config/wt-server.kvm-e2e-install.toml
sudo systemctl disable --now wt-server.service wt-agent-git-gateway.service
make e2e-tests
```

The serialized KVM flow creates devcontainer and host worlds together. It
checks exact host user-data, Byobu and direct host SSH, restart persistence,
app-container recovery, fake-provider Git traffic, and cleanup. A second host
recipe removes WT SSH access. It must remain visible in `error` until the test
deletes it.

To discard an existing installation first, run `make nuke`. This is destructive:
it removes every `wt-*` libvirt domain, the world disks and SQLite registry,
installed images, registry cache, installed services, and encrypted credentials.
Re-run the test-only install command afterward. A clean libvirt inventory can
be confirmed with:

```bash
virsh -c qemu:///system list --all
```
