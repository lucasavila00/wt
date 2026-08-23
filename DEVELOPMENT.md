# Development

WT development requires Ubuntu 24.04 amd64, KVM, `sudo`, Git, OpenSSH client
and server, and stable Rust through rustup. Run setup as a normal user, never
as root.

## Install development dependencies

Install system dependencies with apt, but install Node.js and npm with nvm:

```bash
sudo apt-get update
sudo apt-get install -y shellcheck libvirt-dev
curl --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.3/install.sh | bash
export NVM_DIR="$HOME/.nvm"
[ -s "$NVM_DIR/nvm.sh" ] && . "$NVM_DIR/nvm.sh"
nvm install
nvm use
npm ci
```

The repository pins Node.js `24.19.0` in `.nvmrc` and npm `11.17.0` in
`package.json`. Do not install Node.js or npm with apt.

## Prepare a fresh server

From a root shell in a WT checkout:

```text
scripts/bootstrap-server-user
```

Bootstrap reserves `wt:wt` at UID/GID `1001:1001` and refuses existing account,
numeric-ID, or `/home/wt` ownership conflicts. Reconnect as `wt` before
continuing; server installation rejects any other effective identity.

## Install the server

The development config enables GitHub access for the world gateway. Create its
token file without putting the value in shell history:

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

For the example remote `lab` context, add its server alias to `~/.ssh/config`:

```sshconfig
Host wt-server
    HostName SERVER_ADDRESS
    User wt
```

The server must allow TCP forwarding and reach its libvirt guest network. The
client does not need a direct route to guest addresses.

## Checks

```bash
scripts/cargo test --workspace
make static
```

The repository-wide Cargo settings cap build jobs and test threads at four.
The workspace test command skips the ignored real-system KVM test. Run it only
on a configured Ubuntu/KVM host:

```bash
make e2e-tests
```

## Manual world test

Choose the `local` context when prompted:

```bash
wt new
wt ls
ssh WORLD-direct 'codex --version && wt-tools --help'
ssh WORLD
wt rm WORLD
```

`wt new` opens the world Byobu workspace. The `-direct` alias bypasses the
workspace shell. The golden image supplies Codex and WT uses the server's
login.

## Reset

```bash
make clear
```

This destroys `wt-*` domains and removes worlds, gateway grants, the server
database, generated runtime configuration, and generated SSH inventory. It
keeps verified golden images, installed services and provider credentials,
source credential files, and downloaded image and package artifacts. Rerun
`scripts/install-server --config PATH` afterward.

Use `make nuke` for a full teardown, including installed golden images,
services and credentials, and downloaded artifacts. Neither target removes
source credential files or uninstalls packages and binaries.
