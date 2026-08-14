# wt-devcontainer-git

Scoped Git transport for devcontainer worlds.

The host gateway owns provider credentials and grants. The guest relay carries
requests over vsock. `git-remote-ag` and `ag-git` expose the allowed Git and
provider operations inside the devcontainer.

Keys and provider tokens never enter the guest or container.

Contract: [Devcontainer Git](../../docs/worlds/devcontainer.md#git).
