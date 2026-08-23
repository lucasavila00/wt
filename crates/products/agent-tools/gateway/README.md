# wt-agent-tool-gateway

Scoped Git transport for guests.

The host gateway owns provider credentials and grants. The guest relay carries
requests over vsock. `git-remote-wt-agent` and `wtg tools` expose the allowed Git
and provider operations inside guests.

The same authenticated relay carries advisory Codex session observations into
the shared registry; it validates their Byobu target inside the guest first.

Provider SSH keys and API tokens never enter the guest. Gateway Git prioritizes
availability and does not verify provider SSH host keys; its SSH transport uses
no persistent known-hosts file.
