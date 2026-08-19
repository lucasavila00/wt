# wt-git-proxy

`wt-git-proxy` gives a devcontainer, VM, or CI worker scoped Git access without
copying the upstream credential into it. It uses one OpenSSH `authorized_keys`
file and runs as a forced command, not a daemon.

## How it works

The client connects to the proxy with its own SSH key. OpenSSH checks that key
and forces `wt-git-proxy serve`; the client cannot ask the account to run a
shell command.

The proxy reads the Git command, validates the provider and repository path,
then opens a second SSH connection with the configured upstream key and pinned
host key. Fetches pass through. Pushes must match the configured branch policy.
One denied ref rejects the whole push before it reaches the upstream.

```text
Git client -> OpenSSH -> wt-git-proxy -> OpenSSH -> Git provider
 client key              policy          upstream key
```

The proxy has no listener or background process of its own. Each SSH connection
runs one short-lived proxy process.

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

Everything is in `/etc/wt-git-proxy/config.toml`:

```toml
write_prefix = "agents/"
allowed_branches = ["main"]

[[providers]]
host = "github.com"
user = "git"
port = 22
private_key_file = "/etc/wt-git-proxy/github_ed25519"
known_hosts_file = "/etc/wt-git-proxy/github_known_hosts"

[[providers]]
host = "gitlab.com"
user = "git"
port = 22
private_key_file = "/etc/wt-git-proxy/gitlab_ed25519"
known_hosts_file = "/etc/wt-git-proxy/gitlab_known_hosts"
```

The exact branch list may be empty. The TUI edits this same file.

## Use

When you generate a client key, the TUI prints the bundle location. Its README
tells you where to copy it and gives you the generated SSH name. Then add this
to the client's `~/.ssh/config`:

```sshconfig
Include ~/.ssh/wt-git-proxy/*/config
```

For example, suppose the SSH name is `wt-git-build-vm` and the upstream
repository is WT itself on GitHub. On the client:

```console
git clone wt-git-build-vm:github.com/lucasavila00/wt.git
cd wt
git switch -c agents/improve-readme
git push -u origin agents/improve-readme
```

The provider is part of the path. A real GitLab example is:

```console
git clone wt-git-build-vm:gitlab.com/gitlab-org/gitlab.git
```

Cloning, fetching, and pulling work normally. With the example policy above,
the client may push any branch beginning with `agents/`, plus `main`. A push to
`feature/foo` or a tag is rejected. If one push contains both allowed and
rejected branches, nothing is pushed.

To remove a client's access, rerun `make install-git-server`, choose **Revoke
client**, and select its key. Revocation takes effect on the next connection.

## More detail

- [How a connection moves through the proxy](docs/how-it-works.md)
- [How the write policy works](docs/write-policy.md)
- [Where to pay attention when changing it](docs/maintenance.md)
