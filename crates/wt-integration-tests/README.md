# wt-integration-tests

Cross-crate tests. This package contains no production code.

| Test | Backend |
|------|---------|
| Service behavior | Injected `WorldWorker` |
| Standalone Git proxy | Real Git and two local OpenSSH hops |
| Focused and full lifecycle | Production `wt-libvirt` and local KVM |

Tests use the production API, reservation, background job, lock, registry, log,
and recovery paths. The KVM profiles use the installed devcontainer and host
images and registry cache.

Run from the workspace root:

```text
cargo test --workspace
```

The workspace command skips the ignored real-system tests. Each KVM run uses a
unique gateway port and disposable overlays on the installed golden images, so
it can run beside installed WT services and other test runs.

Use the fast profile while changing VM devices, guest mounts, or Compose binds:

```text
make e2e-tests-fast
```

Run the comprehensive lifecycle profile before release or for changes to the
host recipe, Git gateway, provider API, or recovery behavior:

```text
make e2e-tests-full
```

`make e2e-tests` remains an alias for the full profile.

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
server and agent Git gateway while leaving installed services, images, and
worlds alone:

```bash
scripts/install-server \
    --config examples/server-config/wt-server.kvm-e2e-install.toml
make e2e-tests-fast
```

The fast KVM flow creates minimal devcontainer and host worlds. It checks real
virtiofs mounts, repository-owned Compose binds, cross-world data, restart
recovery, and persistence after world deletion.

The serialized full flow creates devcontainer and host worlds together. It
runs the checked-in host recipe in Byobu with a disposable forwarded agent,
checks the staged bytes, restart persistence, app-container recovery,
fake-provider Git traffic, and cleanup. A second recipe fails cloud-init and
must remain accessible in `error` until the test deletes it.

To discard an existing installation first, run `make nuke`. This is destructive:
it removes every `wt-*` libvirt domain, the world disks and SQLite registry,
installed images, registry cache, installed services, and encrypted credentials.
Re-run the test-only install command afterward. A clean libvirt inventory can
be confirmed with:

```bash
virsh -c qemu:///system list --all
```
