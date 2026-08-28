# wt-agent-tool-gateway

Scoped Git transport for guests.

The host gateway owns provider credentials. The guest relay carries requests
over vsock, and the gateway identifies the world from the accepted socket's
peer CID and its currently active libvirt domain. `git-remote-wt-agent` and
`wtg tools` expose the allowed Git and provider operations inside guests.

The same relay carries server-owned Byobu pane observations into
the shared registry. It reports screen fingerprints, timestamps, each pane's
current working directory, and the branch when that directory has a `.git`
folder. It never reports terminal contents or application lifecycle events.

Provider SSH keys and API tokens never enter the guest. Gateway Git prioritizes
availability and does not verify provider SSH host keys; its SSH transport uses
no persistent known-hosts file.
