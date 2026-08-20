# Development

WT development requires Ubuntu 24.04 amd64, KVM, `sudo`, Git, OpenSSH client
and server, and stable Rust through rustup. Run setup as a normal user, never
as root.

## Prepare a fresh server

When the server has only root SSH access, run this from a root shell in a WT
checkout:

```text
scripts/bootstrap-server-user
```

Reconnect as `wt` before continuing.

## Install the server

The development config enables GitHub. Create its token file without putting
the value in shell history:

```bash
install -d -m 0700 ~/.config/wt/credentials
touch ~/.config/wt/credentials/github.token
chmod 0600 ~/.config/wt/credentials/github.token
${EDITOR:-vi} ~/.config/wt/credentials/github.token
```

Use a classic personal access token with the `repo` scope. The SSH key in
`examples/server-config/wt-server.development.toml` must have repository access,
and its public key and `~/.ssh/known_hosts` must exist. The private key must be
mode `0600`; the public files may be `0600` or `0644`.

Review the config, then install from an interactive terminal:

```bash
scripts/install-server --config examples/server-config/wt-server.development.toml
scripts/install-git-server --config examples/git-proxy-config/wt-git-proxy.development.toml
```

If the key is encrypted, the installer asks for its existing passphrase. If
setup changes group membership, log out, reconnect, and rerun the command.

## Install the client

```bash
scripts/install-client
mkdir -p ~/.wt
cp examples/client-config/wt.development.toml ~/.wt/config.toml
```

The client needs at least one regular public key in `~/.ssh/*.pub`. When the
main SSH configuration is absent, `wt sync` creates it with the managed
inventory include. It reports the required manual change instead of modifying
an existing file.

The example client config includes a local context and a remote `lab` context.
For the remote context, add its server alias to `~/.ssh/config`:

```sshconfig
Host wt-server
    HostName SERVER_ADDRESS
    User wt
```

The server must allow TCP forwarding and reach its libvirt guest network. The
client does not need a direct route to guest addresses.

After CLI-only changes, rebuild and reinstall just the client:

```bash
scripts/install-client
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

## Manual devcontainer test

```bash
wt new
wt ls
ssh jsdev-manual
ssh jsdev-manual-vs
ssh jsdev-manual-host git -C /workspace status
wt rm jsdev-manual
```

Use the `-vs` alias for editor Remote-SSH and open the mounted workspace path.
The project devcontainer includes Codex and logs in as its configured
`remoteUser`, which is `wt`. Run `codex` once inside the session to sign in.

## Manual host test

The client installer's default cloud-init recipe is the manual host-world test;
choose the `local` context when prompted:

```bash
wt new host host-manual
wt ls
ssh host-manual-vs 'command -v diffo && codex --version'
ssh host-manual
wt rm host-manual
```

`wt new host` enters `host-manual` Byobu and runs cloud-init there.
`host-manual-vs` is direct guest SSH. Run `codex` once inside the session to
sign in.

The devcontainer can run the normal Rust checks for this workspace. Neither
environment can run the real KVM E2E from inside a world.

## Reset

```bash
make clear
```

This destroys `wt-*` domains and removes worlds, gateway grants, installed
images, the server database, generated runtime configuration, and generated SSH
inventory. It keeps installed services and provider credentials, source
credential files, downloaded image and package artifacts, and the registry
cache. Rerun `scripts/install-server --config PATH` afterward.

Use `make nuke` for a full teardown, including installed services and
credentials, downloaded artifacts, and registry-cache state. Neither target
removes source credential files or uninstalls packages and binaries.
