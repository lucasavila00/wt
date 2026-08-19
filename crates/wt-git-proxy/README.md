# wt-git-proxy

`wt-git-proxy` gives a devcontainer, VM, or CI worker scoped Git access without
copying the upstream credential into it. It uses one OpenSSH `authorized_keys`
file and runs as a forced command, not a daemon.

## Install

From a WT checkout on Ubuntu 24.04 amd64:

```console
make install-git-server
```

The installer adds the required packages, builds and installs the static
binary, creates the `git-proxy` account, configures sshd, and opens the TUI.
Rerun the same command to upgrade or change configuration.

Before configuring the upstream, place its SSH private key and verified
`known_hosts` file somewhere readable only by `git-proxy`, normally under
`/etc/wt-git-proxy`.

## Configuration

The entire `/etc/wt-git-proxy/config.toml` is the write policy:

```toml
write_prefix = "agents/"
allowed_branches = ["main"]
```

The exact branch list may be empty. The prefix is required and ends in `/`.

The installer opens the TUI to configure the upstream and client keys. Rerun
`make install-git-server` to change them. The upstream credential determines
which repositories are accessible; repository paths pass through unchanged.

## Use

Install a generated bundle at the path in its README, then use its SSH alias:

```console
git clone wt-git-CLIENT:OWNER/REPOSITORY.git
git -C REPOSITORY switch -c agents/task-123
git -C REPOSITORY push -u origin agents/task-123
```

Reads can see every upstream ref. Writes may target the configured prefix or
exact branches. Tags and other refs are rejected, and one denied ref rejects
the whole push. Revoking a key needs no restart or sshd reload.
