# wt-cli

Cargo package for the cockpit **CLI**. Binary name on PATH: **`wt`**.

Full design: [docs/arch/cli.md](../../docs/arch/cli.md).

## Role

- **Transport** — dispatch to local or OpenSSH `wt-server`; JSON over stdio; owner = server OS user
- **Instances** — `new` / `ls` / `rm` / `sync`
- **Names** — `{repo}-{feature}` (e.g. `frontend-checkout-rewrite`)  
- **Output** — name, status, guest IP  

Server helper: [`wt-server`](../wt-server/). Types: [`wt-api`](../wt-api/).

## Commands

```text
wt new <ssh-source> <context.name>
wt logs <name>
wt ls
wt rm <name>
wt sync
```

## Run

```text
cargo run -p wt-cli -- …
```

`new` follows durable provisioning logs and synchronizes managed SSH access only
after the world reaches `running`. `logs` replays and resumes the same output.
`rm` synchronizes after successful deletion.

Configure one or more WT servers in `~/.wt/config.toml`:

```toml
version = 1

[[contexts]]
name = "local"
kind = "bare_metal_local"

[[contexts]]
name = "lab"
kind = "bare_metal_ssh"
host = "wt-lab"
```

`context.world` always identifies a world. The short `world` form works when
it is unambiguous; creating with a short name requires exactly one context.
`wt ls` and `wt sync` query every configured context atomically.

Add this line at the beginning of `~/.ssh/config`, before any `Host` blocks:

```sshconfig
Include ~/.ssh/wt/config
```

`wt sync` owns `~/.ssh/wt/config` and `~/.ssh/wt/known_hosts`. It never edits the user's main SSH config.

After syncing, use stock OpenSSH to enter a world: `ssh <name>`.

The base world alias always attaches to the world's shared tmux or Byobu session,
as selected by the server install config. Shells and processes continue running
across SSH disconnects; all windows and panes enter the primary devcontainer over
SSH. The `-dc` alias is a plain app-container login for VS Code Remote-SSH,
commands, SFTP, and forwarding. The `-host` alias remains a guest recovery login.

For each world, sync always creates `<context>.<name>` aliases and also creates
short aliases when the name is globally unique:

- `<name>` allocates a TTY and attaches to the persistent app session.
- `<name>-dc` is unrestricted app-container SSH and the VS Code Remote-SSH target.
- `<name>-host` is unrestricted guest SSH for recovery.
