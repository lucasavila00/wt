# Host world

`cloud-init.yaml` creates a development environment for this repository. It
installs Rust, Clippy, rustfmt, libvirt headers, and Codex; clones WT into
`~/wt`; and runs strict workspace Clippy before creation completes.

```text
wt new host examples/host-world/cloud-init.yaml
ssh CONTEXT.NAME
ssh CONTEXT.NAME-vs 'cd ~/wt && cargo clippy --workspace --all-targets -- -D warnings'
ssh CONTEXT.NAME-vs codex --version
```

The generated aliases log in as `wt`. The regular alias attaches to Byobu; use
`-vs` for direct SSH and remote commands. Run `codex` interactively once to
sign in. Git and `ag-git` use the WT gateway without placing provider
credentials in the world.

The environment can run normal workspace checks, but not the real KVM E2E.
