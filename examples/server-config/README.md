# Server config

The sample is an install input for:

```text
scripts/install-server --config PATH
```

Copy it outside this directory and review every value. Setup writes the strict
runtime config to `/etc/wt/server.toml`. Keep the input for reinstalling the same
configuration.

Each `agent_git` provider names an API-token file, SSH key pair, and trusted
host-key file. Paths may be absolute or start with `~/`. The installer validates
them and stores encrypted copies for the gateway; worlds never receive them.
`agent_git.vsock_port` is the private gateway endpoint shared by the server and
world relays. Installed services use the configured value. Development and E2E
processes may override it with `WT_AGENT_GIT_VSOCK_PORT`.
The `image` section names separate devcontainer and host images in one
directory. They cannot use the same file.
Each `[[shared_folders]]` entry maps an absolute server directory to a path
relative to `/home/wt` in every retained VM. The sample shares only Codex
sessions and Claude Code projects. Repository-owned Docker Compose services may
bind those VM paths into their own container user home; WT does not add
container mounts.
Changing strict server settings requires `make nuke` followed by reinstalling.

`wt-server.kvm-e2e-install.toml` is different: it prepares a clean, dedicated
KVM test host with disposable provider fixtures. It must not be used to run a
real WT server. See the [integration-test instructions](../../crates/wt-integration-tests/README.md#clean-kvm-test-host).
