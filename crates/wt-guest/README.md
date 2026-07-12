# wt-guest

Guest-side programs injected into each world by WT.

The crate builds `wt-app-shell`, the remote command used by managed SSH aliases
to enter the primary devcontainer. It finds the running container for
`/workspace`, resolves the checkout's mount path and configured devcontainer
user, then starts an interactive shell in that container.

The helper is built and installed by [`wt-server-setup`](../wt-server-setup/).
SSH aliases are managed by [`wt-cli`](../wt-cli/). See the
[CLI architecture](../../docs/arch/cli.md) for the complete connection flow.
