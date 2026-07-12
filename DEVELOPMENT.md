# Local development

Run these commands from the repository root on Ubuntu 24.04 amd64 with hardware
virtualization enabled. KVM is required.

## Install the server

Review the Git identity, SSH keys, resource sizes, and paths in
`config/wt-server.development.toml`, then run:

```bash
scripts/install-server --config config/wt-server.development.toml
```

The script installs the required host packages and invokes `sudo`. Run it in an
interactive terminal. If it adds your user to the `docker`, `libvirt`, or `kvm` group, log
out, log back in, and run the same command again.

The install also starts the shared registry cache and preloads the images listed
in `registry_cache.preload_images`. Re-run the installer after a full clear when
changing the strict server configuration.

## Configure the client

Create `~/.wt/config.toml`:

```toml
version = 1

[[contexts]]
name = "local"
kind = "bare_metal_local"
```

Add this before any `Host` blocks in `~/.ssh/config`:

```sshconfig
Include ~/.ssh/wt/config
```

## Run the checks

```bash
cargo fmt --all
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

The workspace test always includes the real KVM acceptance test.

## Exercise a world

```bash
wt new git@github.com:lucasavila00/jsdev-sample.git jsdev-manual
wt ls
wt sync
ssh jsdev-manual
```

Inside the devcontainer:

```bash
pwd
git status
exit
```

Use the host alias for guest SSH, explicit commands, SCP, or VS Code Remote SSH:

```bash
ssh jsdev-manual-host
ssh jsdev-manual-host git -C /workspace status
```

Remove the world when finished:

```bash
wt rm jsdev-manual
wt ls
```

## Reset the server

To destroy every `wt-*` domain and remove all installed WT development state:

```bash
make clear
```

Re-run the install command afterward. `make clear` delegates to
`scripts/clear-server`; it does not uninstall packages or binaries.
