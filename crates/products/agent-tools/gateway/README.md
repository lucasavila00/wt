# wt-agent-tool-gateway

Scoped Git transport for guests.

The host gateway owns provider credentials and grants. The guest relay carries
requests over vsock. `git-remote-wt-agent` and `wtg tools` expose the allowed Git
and provider operations inside guests.

The same authenticated relay carries server-owned Byobu pane observations into
the shared registry. It reports only screen fingerprints and timestamps, never
terminal contents or application lifecycle events.

Provider SSH keys and API tokens never enter the guest. Gateway Git prioritizes
availability and does not verify provider SSH host keys; its SSH transport uses
no persistent known-hosts file.
