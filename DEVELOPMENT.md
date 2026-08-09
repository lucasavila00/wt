# Development

Use the devcontainer for normal Rust development and tests. Installing a local
server and running the end-to-end test require Ubuntu 24.04 amd64 with KVM
enabled.

## Install the local server

The development config enables GitHub. Before installing, create
`~/.config/wt/credentials/github.token` as a nonempty `0600` file and make sure
the SSH key configured in the example can read and write the repositories you
will use. Its public key and `~/.ssh/known_hosts` must also exist.

Review `examples/server-config/wt-server.development.toml`, then run from an
interactive terminal:

```bash
scripts/install-server --config examples/server-config/wt-server.development.toml
```

Run as a normal user. The installer asks for the SSH key passphrase when needed,
checks that the key pair matches, and gives encrypted copies of the token and
unlocked key to the local gateway service. If setup changes group membership,
log out, log back in, and rerun it.

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
wt new
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

This destroys `wt-*` domains and removes worlds, gateway grants, the golden
image, the server database, and generated SSH inventory. It keeps the server
installation, provider credentials, configuration, Ubuntu source image, and
registry cache.

Use `make nuke` for a full teardown, including configuration and registry-cache
state. Neither target uninstalls packages or binaries.
