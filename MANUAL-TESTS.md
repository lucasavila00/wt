# Manual tests

This is the normal Era 1.5 operator smoke test. Automated and diagnostic tests are documented in [TESTS.md](./TESTS.md).

## Install the local site

From the `wt` checkout on Ubuntu 24.04 amd64:

```bash
scripts/install-site --config config/wt-local.development.toml
```

The first run may add the current user to the `libvirt` and `kvm` groups. If it asks you to log out, log back in and run the same command again.

## Create a world

The default identity is `~/.ssh/id_ed25519`. If it is encrypted, `wt new` asks for its passphrase.

```bash
wt new git@github.com:lucasavila00/jsdev-sample.git jsdev-manual
```

The command finishes after the repository is cloned, its devcontainer is running, and SSH access is ready.

## Use the world

```bash
wt ls
wt ssh jsdev-manual
```

Inside the world, the checkout is at `/workspace`:

```bash
cd /workspace
git status
devcontainer exec --workspace-folder /workspace node --version
exit
```

Normal Git commands from the guest or devcontainer use the same SSH identity selected by `wt new`.

## Remove the world

```bash
wt rm jsdev-manual
wt ls
```

`wt new` and `wt rm` sync the managed SSH configuration automatically.
