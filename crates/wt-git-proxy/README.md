# wt-git-proxy

`wt-git-proxy` gives a devcontainer, cloud VM, or CI worker scoped SSH Git access
without its upstream credential. It is a forced command using one `authorized_keys` file.

## Install

Build from this workspace and create a dedicated account:

```console
cargo build --release --locked -p wt-git-proxy
sudo install -m 0755 target/release/wt-git-proxy /usr/local/bin/wt-git-proxy
sudo adduser --system --group --home /var/lib/wt-git-proxy --shell /bin/sh git-proxy
sudo usermod --password '*NP*' git-proxy
sudo install -d -o git-proxy -g git-proxy -m 0700 /etc/wt-git-proxy /var/lib/wt-git-proxy
```

`*NP*` keeps password login impossible without making OpenSSH reject the account
as locked. Install an upstream SSH private key and verified `known_hosts` file
under `/etc/wt-git-proxy`, owned by `git-proxy` and mode `0600`.

## Configuration

`/etc/wt-git-proxy/config.toml`:

```toml
version = 1
authorized_keys_file = "/var/lib/wt-git-proxy/authorized_keys"
executable = "/usr/local/bin/wt-git-proxy"
write_prefix = "refs/heads/agents/"
allowed_branches = ["refs/heads/main"]

[client]
host = "git-proxy.example.com"
port = 22
user = "git-proxy"
host_key_file = "/etc/ssh/ssh_host_ed25519_key.pub"

[[upstreams]]
name = "github"
host = "github.com"
user = "git"
private_key_file = "/etc/wt-git-proxy/upstream_ed25519"
known_hosts_file = "/etc/wt-git-proxy/upstream_known_hosts"

[[repositories]]
path = "/acme/api.git"
upstream = "github"
upstream_path = "acme/api.git"
```

The exact branch list may be empty. The prefix is required and ends in `/`.
Add more upstreams or repositories as needed; either may use a custom SSH port.

The TUI edits mappings, policy, and client keys:

```console
sudo -u git-proxy /usr/local/bin/wt-git-proxy \
  --config /etc/wt-git-proxy/config.toml tui
```

## OpenSSH

Add `/etc/ssh/sshd_config.d/wt-git-proxy.conf`:

```sshconfig
Match User git-proxy
    AuthorizedKeysFile /var/lib/wt-git-proxy/authorized_keys
    PasswordAuthentication no
    KbdInteractiveAuthentication no
    AllowAgentForwarding no
    AllowTcpForwarding no
    X11Forwarding no
    PermitTTY no
```

Then validate and reload:

```console
sudo sshd -t
sudo systemctl reload ssh
```

## Clients

Authorize an existing public key or generate a bundle with a private key,
pinned host key, SSH config, and clone command. Transfer it, follow its README,
then delete the server copy; the server retains only the public key.

```console
git clone wt-git-CLIENT:/acme/api.git
git -C api switch -c agents/task-123
git -C api push -u origin agents/task-123
```

Reads can see every ref. Writes may target the configured prefix or exact
branches; tags and other refs are rejected, and one denied ref rejects the
whole push. Revoke a client in the TUI; no process restart or sshd reload is
needed.
