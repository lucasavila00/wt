# Agents

- Read `docs/impl/README.md` and `docs/arch/README.md` first.
- Era 1.5: Ubuntu 24.04 amd64, local CLI, libvirt/KVM, Git/Compose recipe, and SSH access to guests.
- Guest SSH is in scope; SSH transport to `wt-local`, contexts, runtime env overrides, and emulation fallback are not.
- Keep `wt-local` slim. Host setup belongs in `wt-setup`. Real-system tests belong in `wt-integration-tests`.
- Site config is strict and complete at `/etc/wt/local.toml`. Fail on drift.
- Run `cargo fmt --all`, `cargo test --workspace`, and `cargo clippy --workspace --all-targets -- -D warnings`.
