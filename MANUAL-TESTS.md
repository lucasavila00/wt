# Manual tests

This is the normal Era 1.5 operator smoke test. Automated and diagnostic tests are documented in [TESTS.md](./TESTS.md).

## Install the local site

From the `wt` checkout on Ubuntu 24.04 amd64:

```bash
scripts/install-site --config config/wt-local.development.toml
```

The first run may add the current user to the `libvirt` and `kvm` groups. If it asks you to log out, log back in and run the same command again.
The installer also grants `libvirt-qemu` search-only ACL access to the worlds
directory so libvirt can traverse it without exposing worlds to other users.

## Prepare the automated KVM test image

Before running `cargo test --workspace` on this workstation, build the separate
integration-test image cache once:

```bash
scripts/prepare-test-image --config config/wt-local.development.toml
```

Run this command in an interactive terminal because it invokes `sudo`. Re-run
it after rebuilding the production golden image or changing
`crates/wt-integration-tests/fixture-images.txt`.

This cache is used only by the automated KVM test. It does not change the
production golden image or the ordinary `wt new` manual workflow below. See
[TESTS.md](./TESTS.md) for cache paths, validation, and diagnostic commands.

## Create a world

Once per workstation, add this line at the beginning of `~/.ssh/config`, before any `Host` blocks:

```sshconfig
Include ~/.ssh/wt/config
```

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

The regular alias opens `/bin/sh` as the devcontainer's configured user in its workspace:

```bash
pwd
git status
exit
```

Use the `-host` alias for a normal guest SSH session, explicit remote commands, SCP, or VS Code Remote SSH:

```bash
ssh jsdev-manual-host
ssh jsdev-manual-host git -C /workspace status
```

Normal Git commands from the guest or devcontainer use the same SSH identity selected by `wt new`.

## Remove the world

```bash
wt rm jsdev-manual
wt ls
```

`wt new` and `wt rm` sync the managed SSH configuration automatically.
