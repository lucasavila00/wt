# wt-git-proxy

`wt-git-proxy` gives a devcontainer, VM, or CI worker scoped Git access without
copying the upstream credential into it. It uses one OpenSSH `authorized_keys`
file and runs as a forced command, not a daemon.

## Install

From a WT checkout on Ubuntu 24.04 amd64:

```console
make install-git-server
```

The installer adds the required packages, builds and installs the
binary, creates the `git-proxy` account, configures sshd, and opens the TUI.
Rerun the same command to upgrade or change configuration.

Before configuring the upstream, place its SSH private key and verified
`known_hosts` file somewhere readable only by `git-proxy`, normally under
`/etc/wt-git-proxy`.

## Configuration

The entire `/etc/wt-git-proxy/config.toml` is the write policy:

```toml
write_prefix = "agents/"
allowed_branches = ["main"]
```

The exact branch list may be empty. The prefix is required and ends in `/`.

The installer opens the TUI to choose one upstream host and its SSH key. It
writes those details to `/etc/wt-git-proxy/upstream.ssh_config`; they are not
part of the policy TOML. Rerun `make install-git-server` to change them.

The TUI's upstream host supplies the missing part of the SSH URL. For example:

- upstream `git@github.com` + path `lucasavila00/wt.git` means
  `git@github.com:lucasavila00/wt.git`;
- upstream `git@gitlab.com` + path `gitlab-org/gitlab.git` means
  `git@gitlab.com:gitlab-org/gitlab.git`.

GitHub, GitLab, and self-hosted SSH Git servers work the same way. One proxy
installation points to one upstream host.

## Use

When you generate a client key, the TUI prints the bundle location and an SSH
name for that client. Copy the bundle to the client's
`~/.ssh/wt-git-proxy/NAME` directory and add this to `~/.ssh/config`:

```sshconfig
Include ~/.ssh/wt-git-proxy/*/config
```

For example, suppose the SSH name is `wt-git-build-vm` and the upstream
repository is WT itself on GitHub. On the client:

```console
git clone wt-git-build-vm:lucasavila00/wt.git
cd wt
git switch -c agents/improve-readme
git push -u origin agents/improve-readme
```

For a proxy configured for GitLab, a real read-only example is:

```console
git clone wt-git-build-vm:gitlab-org/gitlab.git
```

Cloning, fetching, and pulling work normally. With the example policy above,
the client may push any branch beginning with `agents/`, plus `main`. A push to
`feature/foo` or a tag is rejected. If one push contains both allowed and
rejected branches, nothing is pushed.

To remove a client's access, rerun `make install-git-server`, choose **Revoke
client**, and select its key. Revocation takes effect on the next connection.
