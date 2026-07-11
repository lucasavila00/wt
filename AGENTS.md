# Agents

- Read `docs/impl/README.md` and `docs/arch/README.md` first.
- Era 1 only: Ubuntu 24.04 amd64, local CLI, libvirt/KVM.
- No SSH, recipes, contexts, runtime env overrides, or emulation fallback.
- Keep `wt-local` slim. Host setup belongs in `wt-setup`. Real-system tests belong in `wt-integration-tests`.
- Site config is strict and complete at `/etc/wt/local.toml`. Fail on drift.
- Run `cargo fmt --all`, `cargo test --workspace`, and `cargo clippy --workspace --all-targets -- -D warnings`.
