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
