# Getting started

WT servers require Ubuntu 24.04 amd64, KVM, `sudo`, Git, and stable Rust. Run
setup as a normal server user, never as root.

## Prepare a fresh server

When a hosted server has only root SSH access, run this from a root shell in a
WT checkout:

```text
scripts/bootstrap-server-user
```

It creates the fixed `wt` account, copies root's authorized keys, and grants the
trusted account the sudo, Docker, libvirt, and KVM access needed by setup.
Reconnect as `wt` before continuing.

## Install the server

```text
cp examples/server-config/wt-server.development.toml ./server.toml
scripts/install-server --config ./server.toml
```

Review capacity limits, image resources, registry-cache hosts, and the agent
Git provider in the install input. The current installer requires one provider
for devcontainer worlds. Its API token and SSH key stay on the server; host
worlds never receive them.

The example expects the token at
`~/.config/wt/credentials/github.token`. Create it without putting the value in
shell history:

```text
install -d -m 0700 ~/.config/wt/credentials
touch ~/.config/wt/credentials/github.token
chmod 0600 ~/.config/wt/credentials/github.token
${EDITOR:-vi} ~/.config/wt/credentials/github.token
```

The current GitHub integration uses a classic personal access token with the
`repo` scope. Give it an expiry, repository access, and organization SSO access
appropriate for the repositories this server manages.

If setup changes group membership, log out, reconnect, and rerun the same
install command. Keep the install input for later reinstalls.

Moving an existing installation to the first-class world-kind schema requires
the destructive reset described in [Server operations](./docs/guides/server.md#reset).

## Configure the client

On the workstation, install the client from a WT checkout:

```text
scripts/install-client
```

```text
mkdir -p ~/.wt
cp examples/client-config/wt.development.toml ~/.wt/config.toml
mkdir -p ~/.ssh
chmod 700 ~/.ssh
```

Put this before other `Host` blocks in `~/.ssh/config`:

```sshconfig
Include ~/.ssh/wt/config
```

For a remote context, give the WT server a normal OpenSSH alias and reference it
from `~/.wt/config.toml`:

```sshconfig
Host wt-server
    HostName SERVER_ADDRESS
    User wt
```

```toml
version = 1

[[contexts]]
name = "lab"
kind = "bare_metal_ssh"
host = "wt-server"
```

The server must allow TCP forwarding and reach its libvirt guest network. The
workstation does not need a direct route to guest addresses.

## Create worlds

Create a repository devcontainer and complete setup:

```text
wt new
```

`wt new` enters the setup session itself. Reconnect later with
`ssh CONTEXT.NAME`.

Create a raw Ubuntu host from cloud-init:

```text
wt new host ./host.yaml
ssh lab.ubuntu
ssh lab.ubuntu-vs
```

The first host alias attaches to Byobu; `-vs` is direct guest SSH.

Then use `wt ls`, `wt start NAME`, and `wt rm NAME` for retained worlds.
`wt code NAME` is devcontainer-only.

See [WT documentation](./docs/README.md) for world contracts and internals.
