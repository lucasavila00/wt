# wt-agent-tool-gateway

Scoped Git transport for retained worlds.

The host gateway owns provider credentials and grants. The guest relay carries
requests over vsock. `git-remote-wt-agent` and `wt-tools` expose the allowed Git and
provider operations inside devcontainer and host worlds.

The same authenticated relay carries advisory Codex session observations into
the shared registry; it validates their Byobu target inside the guest first.

Provider SSH keys and API tokens never enter the guest or container.

Contracts: [Devcontainer Git](../../docs/worlds/devcontainer.md#git) and
[host worlds](../../docs/worlds/host.md).
