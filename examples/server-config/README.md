# Server config

The sample is an install input for:

```text
scripts/install-server --config PATH
```

Copy it outside this directory and review every value. Setup writes the strict
runtime config to `/etc/wt/server.toml`. Keep the input for reinstalling the same
configuration.

Each `agent_tools` provider names an API-token file, SSH key pair, and trusted
host-key file. Paths may be absolute or start with `~/`. The installer validates
them and stores encrypted copies for the gateway; worlds never receive them.
`agent_tools.vsock_port` is the private gateway endpoint shared by the server and
world relays. Installed services use the configured value. Development and E2E
processes may override it with `WT_AGENT_TOOL_VSOCK_PORT`.
The `image` section names one retained-world image used by devcontainer and
host worlds.
Before installation, the server's `wt` user must log in to Codex and own a
regular, non-symlink `/home/wt/.codex/auth.json`. Codex integration has no
configuration: every retained world receives the server-backed sessions and
read-only login, and devcontainer worlds inject them into their primary
container automatically.
Changing strict server settings requires `make nuke` followed by reinstalling.

`wt-server.kvm-e2e-install.toml` is different: it prepares a clean, dedicated
KVM test host with disposable provider fixtures. It must not be used to run a
real WT server. See the [integration-test instructions](../../crates/tests/end-to-end/README.md#clean-kvm-test-host).
