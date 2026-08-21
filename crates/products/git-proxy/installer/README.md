# wt-git-proxy-installer

Ubuntu 24.04 amd64 installer for the standalone WT Git proxy.

It validates the install configuration and provider SSH credentials, installs
the `git-proxy` account and forced-command service, and runs the dashboard used
to authorize or revoke agent keys.

Host file, command, and SSH credential handling comes from
`wt-installer-support`. Proxy request handling stays in `wt-git-proxy`.

Install and operation: [wt-git-proxy](../service/README.md).
