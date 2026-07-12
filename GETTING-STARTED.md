# Getting started

Use Ubuntu 24.04 amd64. KVM must work. You need `sudo`.

WT builds from source. Run setup as your user. Do not run it as root.

## Install on this machine

Install Git and stable Rust.

Clone WT:

```bash
git clone https://github.com/lucasavila00/wt.git
cd wt
```

Set up GitHub/GitLab SSH access:

```bash
ssh -T git@github.com
```

Copy the sample, edit it, then pass it to the installer:

```bash
cp config/wt-server.development.toml ./server.toml
nano ./server.toml
```

Check these values:

- `git.identity_file`: encrypted private key used to clone repositories.
- `git.known_hosts_file`: trusted Git server host keys used when WT clones.
- `guest.ssh_authorized_keys_file`: public keys that worlds accept for SSH
  login.
- `guest.memory_mib`: maximum RAM per world.
- `guest.vcpus`: virtual CPUs per world. Host CPUs are shared.
- `guest.disk_gib`: maximum disk per world. Disk grows as used.
- `registry_cache.registries`: registry hosts to cache. Other registries pass
  through without caching. Images from these registries are cached on first
  pull. Tags are checked upstream on every pull; image blobs are reused from
  cache.

See [Libvirt/KVM backend](docs/arch/bare-metal-agent.md) and
[registry cache](docs/arch/registry-cache.md).

Install:

```bash
scripts/install-server --config ./server.toml
```

If it tells you to log out, log out. Log back in. Run it again:

```bash
cd ~/wt
scripts/install-server --config ./server.toml
```

The installer materializes `/etc/wt/server.toml` from `./server.toml`. Keep
`./server.toml` until install finishes (including any re-run after login), and
for a later clear + reinstall if you want the same settings. Installed server
state is strict; a differing materialized config fails reinstall.

## Configure the local client

Create the client config:

```bash
mkdir -p ~/.wt
nano ~/.wt/config.toml
```

Put this in it:

```toml
version = 1

[[contexts]]
name = "local"
kind = "bare_metal_local"
```

Open your SSH config:

```bash
mkdir -p ~/.ssh
chmod 700 ~/.ssh
nano ~/.ssh/config
```

Put this before every `Host` block:

```sshconfig
Include ~/.ssh/wt/config
```

## Create a world

```bash
wt new git@github.com:lucasavila00/jsdev-sample.git local.jsdev-manual
wt ls
ssh local.jsdev-manual
```

Enter the guest instead of the devcontainer:

```bash
ssh local.jsdev-manual-host
```

Remove the world:

```bash
wt rm local.jsdev-manual
```

## Use a remote server

Install the server with the steps above. Do not add the local context on the
client.

On the client, add a normal server alias to `~/.ssh/config`:

```sshconfig
Include ~/.ssh/wt/config

Host wt-server
    HostName SERVER_ADDRESS
    User SERVER_USER
```

Before server installation, copy the client's public key to the server:

```bash
scp ~/.ssh/id_ed25519.pub wt-server:~/.ssh/wt-client.pub
```

On the server, set this in `./server.toml` before installation:

```toml
[guest]
ssh_authorized_keys_file = "~/.ssh/wt-client.pub"
```

Keep the other `[guest]` values from the sample.

On the client, install Git, stable Rust, and an OpenSSH client. Then install WT:

```bash
git clone https://github.com/lucasavila00/wt.git
cargo install --path wt/crates/wt-cli
```

Test the server:

```bash
printf '%s\n' '{"protocol_version":1,"operation":"list"}' | ssh wt-server wt-server api
```

Create `~/.wt/config.toml`:

```toml
version = 1

[[contexts]]
name = "lab"
kind = "bare_metal_ssh"
host = "wt-server"
```

Use it:

```bash
wt new git@github.com:lucasavila00/jsdev-sample.git lab.jsdev-manual
wt ls
ssh lab.jsdev-manual
wt rm lab.jsdev-manual
```
