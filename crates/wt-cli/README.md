# wt-cli

Cargo package for the cockpit **CLI**. Binary name on PATH: **`wt`**.

Full design: [docs/arch/cli.md](../../docs/arch/cli.md).

## Role

- **Context** — `bare_metal_local` in Era 1  
- **Transport** — spawn local `wt-local`; JSON over stdio; owner = OS user  
- **Instances** — `new` / `ls` / `rm`  
- **Names** — `{repo}-{feature}` (e.g. `frontend-checkout-rewrite`)  
- **Output** — name, status, guest IP  

Site server: [`wt-local`](../wt-local/). Types: [`wt-api`](../wt-api/).

## Commands (target)

```text
wt new <source> <name>
wt ls
wt rm <name>
```

## Run

```text
cargo run -p wt-cli -- …
```

## Status

Era 1 implementation in progress.
