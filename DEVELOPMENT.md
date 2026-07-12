# Development

Use the devcontainer for normal Rust development and tests. Installing a local
server and running the end-to-end test require Ubuntu 24.04 amd64 with KVM
enabled.

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

After CLI-only changes, rebuild and reinstall just the local client:

```bash
scripts/install-client
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

`cargo test --workspace` skips the ignored real-system KVM test. Run it only on
a configured Ubuntu/KVM host:

```bash
make e2e-tests
```

## Manual test

```bash
wt new git@github.com:lucasavila00/jsdev-sample.git jsdev-manual
wt ls
ssh jsdev-manual
ssh jsdev-manual-vs
ssh jsdev-manual-host git -C /workspace status
wt rm jsdev-manual
```

Use the `-vs` alias for editor Remote-SSH and open the mounted workspace path.

## Reset

```bash
make clear
```

This destroys `wt-*` domains and removes WT development state. It does not
uninstall packages or binaries.
