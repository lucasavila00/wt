# wt-cli

Cargo package for the cockpit **CLI**. Binary name on PATH: **`wt`**.

Full design: [docs/arch/cli.md](../../docs/arch/cli.md).

## Role

- **Transport** — dispatch to local or OpenSSH `wt-local`; JSON over stdio; owner = server OS user
- **Instances** — `new` / `ls` / `rm` / `sync` / `ssh`  
- **Names** — `{repo}-{feature}` (e.g. `frontend-checkout-rewrite`)  
- **Output** — name, status, guest IP  

Site server: [`wt-local`](../wt-local/). Types: [`wt-api`](../wt-api/).

## Commands

```text
wt new <ssh-source> <context.name> [--ref <ref>]
wt ls
wt rm <name>
wt sync
wt ssh <name>
```

## Run

```text
cargo run -p wt-cli -- …
```

`new` and `rm` always synchronize managed SSH access records.

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

For each world, sync always creates `<context>.<name>` aliases and also creates
short aliases when the name is globally unique:

- `<name>` allocates a TTY and enters the primary app container with `docker exec -it`.
- `<name>-host` is unrestricted guest SSH for commands, SCP, and VS Code Remote SSH.
