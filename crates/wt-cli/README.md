# wt-cli

Cargo package for the cockpit **CLI**. Binary name on PATH: **`wt`**.

Full design: [docs/arch/cli.md](../../docs/arch/cli.md).

## Role

- **Transport** — spawn local `wt-local`; JSON over stdio; owner = OS user  
- **Instances** — `new` / `ls` / `rm` / `sync` / `ssh`  
- **Names** — `{repo}-{feature}` (e.g. `frontend-checkout-rewrite`)  
- **Output** — name, status, guest IP  

Site server: [`wt-local`](../wt-local/). Types: [`wt-api`](../wt-api/).

## Commands (target)

```text
wt new <ssh-source> <name> [--ref <ref>] [--identity PATH]
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

Add this line at the beginning of `~/.ssh/config`, before any `Host` blocks:

```sshconfig
Include ~/.ssh/wt/config
```

`wt sync` owns `~/.ssh/wt/config` and `~/.ssh/wt/known_hosts`. It never edits the user's main SSH config.

For each world, sync creates two aliases:

- `<name>` allocates a TTY and enters the primary app container with `docker exec -it`.
- `<name>-host` is unrestricted guest SSH for commands, SCP, and VS Code Remote SSH.
