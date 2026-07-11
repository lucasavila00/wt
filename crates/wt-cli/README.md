# wt-cli

Cargo package for the cockpit **CLI**. Binary name on PATH: **`wt`**.

Full design: [docs/arch/cli.md](../../docs/arch/cli.md).

## Role

- **Contexts** — sum type: **`bare_metal_ssh`** and **`bare_metal_local`**; later `k8s`; `--context`  
- **Transport** — spawn helper: `ssh … -- wt-local …` **or** local `wt-local …`; same JSON stdio; owner = SSH or OS user  
- **Instances** — `new` / `ls` / `rm` for **my** envs on a multi-user host  
- **Names** — `{repo}-{feature}` (e.g. `frontend-checkout-rewrite`)  
- **World SSH** — print Host on create; **`wt sync`** → managed `~/.config/wt/ssh_config`; optional **`wt ssh`** into the **guest**  

Site server: [`wt-local`](../wt-local/). Types: [`wt-api`](../wt-api/).

## Commands (target)

```text
wt context list|use|show
wt new <source> <name>
wt ls
wt rm <name>
wt sync
wt ssh <name>
```

## Run

```text
cargo run -p wt-cli -- …
```

## Status

Topology only; not implemented yet.
