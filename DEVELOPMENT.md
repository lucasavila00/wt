# Development

Run development and tests on Ubuntu 24.04 amd64 with KVM enabled.

## Install the local server

Review `examples/server-config/wt-server.development.toml`, then run:

```bash
scripts/install-server --config examples/server-config/wt-server.development.toml
```

Run as a normal user in an interactive terminal. If setup changes group
membership, log out, log back in, and rerun it.

Install the local client config:

```bash
mkdir -p ~/.wt
cp examples/client-config/wt.development.toml ~/.wt/config.toml
```

Add this before every `Host` block in `~/.ssh/config`:

```sshconfig
Include ~/.ssh/wt/config
```

## Checks

```bash
cargo fmt --all
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

`cargo test --workspace` includes the real libvirt/KVM test.

## Manual test

```bash
wt new git@github.com:lucasavila00/jsdev-sample.git jsdev-manual
wt ls
ssh jsdev-manual
ssh jsdev-manual-dc
ssh jsdev-manual-host git -C /workspace status
wt rm jsdev-manual
```

Use the `-dc` alias for VS Code Remote-SSH and open the mounted workspace path.

## Reset

```bash
make clear
```

This destroys `wt-*` domains and removes WT development state. It does not
uninstall packages or binaries.
