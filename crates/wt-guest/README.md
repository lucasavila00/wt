# wt-guest

Guest-side programs injected into each world by WT.

The crate builds single-purpose guest helpers. `wt-app-shell` is the remote
command used by managed SSH aliases; it attaches to the world's configured
persistent tmux or Byobu session. Every window and pane runs `wt-app-pane`,
which finds the running container for `/workspace`, resolves the checkout's
mount path, configured user, and address, then enters it over SSH. `wt-app-proxy`
exposes the same dynamic target to client OpenSSH without publishing a Docker port.

The helpers are built and installed by [`wt-server-setup`](../wt-server-setup/).
SSH aliases are managed by [`wt-cli`](../wt-cli/). See the
[CLI architecture](../../docs/arch/cli.md) for the complete connection flow.
