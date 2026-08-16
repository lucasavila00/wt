# wt-devcontainer-git

Scoped Git transport for retained worlds.

The host gateway owns provider credentials and grants. The guest relay carries
requests over vsock. `git-remote-ag` and `ag-git` expose the allowed Git and
provider operations inside devcontainer and host worlds.

Provider SSH keys and API tokens never enter the guest or container.

Contracts: [Devcontainer Git](../../docs/worlds/devcontainer.md#git) and
[host worlds](../../docs/worlds/host.md).
