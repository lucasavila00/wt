# Getting started

WT servers require Ubuntu 24.04 amd64, KVM, `sudo`, Git, and stable Rust. Run
setup as a normal user from a source checkout.

## Install a server

```bash
git clone https://github.com/lucasavila00/wt.git
cd wt
cp server-config/wt-server.development.toml ./server.toml
```

Edit `server.toml`. At minimum, check:

- `git.identity_file`: encrypted SSH key used to clone repositories.
- `git.known_hosts_file`: trusted Git host keys.
- `guest.ssh_authorized_keys_file`: public keys allowed into worlds.
- `guest.session`: `tmux` or `byobu`.
- `guest.memory_mib`, `guest.vcpus`, and `guest.disk_gib`.
- `registry_cache.registries`: registry hosts whose public images are cached.

Install:

```bash
scripts/install-server --config ./server.toml
```

If setup changes group membership, log out, log back in, and run the same command
again. Setup writes the strict runtime configuration to `/etc/wt/server.toml`.
Keep the install input for future reinstalls.

## Configure a local client

Install the local client config:

```bash
mkdir -p ~/.wt
cp client-config/wt.development.toml ~/.wt/config.toml
```

Add this before every `Host` block in `~/.ssh/config`:

```bash
mkdir -p ~/.ssh
chmod 700 ~/.ssh
```

```sshconfig
Include ~/.ssh/wt/config
```

## Create and enter a world

```bash
wt new git@github.com:org/repo.git local.repo-feature
wt ls
ssh local.repo-feature
```

Managed aliases:

| Alias | Target |
|-------|--------|
| `NAME` | Persistent app session |
| `NAME-dc` | Devcontainer; use for VS Code Remote-SSH |
| `NAME-host` | Guest shell and recovery |

Remove the world:

```bash
wt rm local.repo-feature
```

App images must be Debian- or Ubuntu-derived and support `apt`.

## Use a remote server

Give the server a normal OpenSSH alias on the client:

```sshconfig
Include ~/.ssh/wt/config

Host wt-server
    HostName SERVER_ADDRESS
    User SERVER_USER
```

Before server setup, copy the client's public key:

```bash
scp ~/.ssh/id_ed25519.pub wt-server:~/.ssh/wt-client.pub
```

Set this path in the server install input, then install the server:

```toml
[guest]
ssh_authorized_keys_file = "~/.ssh/wt-client.pub"
```

Keep the other `[guest]` values from the sample.

Install the client:

```bash
git clone https://github.com/lucasavila00/wt.git
cargo install --path wt/crates/wt-cli
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
wt new git@github.com:org/repo.git lab.repo-feature
ssh lab.repo-feature
```

Client-to-server, server-to-Git, and client-to-world SSH keys are separate roles.
The same key may serve more than one role.
