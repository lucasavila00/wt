# How a connection moves through the proxy

There are two SSH connections and two different keys.

```text
client -- client key --> proxy -- upstream key --> Git provider
```

1. The client connects as `git-proxy`.
2. OpenSSH finds the client key in the managed `authorized_keys` file.
3. That key forces `wt-git-proxy serve`. It cannot open a normal shell.
4. Git asks for `git-upload-pack` or `git-receive-pack` and includes a path such
   as `github.com/team/project.git`.
5. The proxy checks the command and path, finds the provider named by the first
   path component, and opens the upstream SSH connection.
6. `wt-git-core` carries the Git traffic. For a push, it checks the write
   policy before it sends the client's changes upstream.

The client never receives the upstream private key. The upstream never sees the
client key.

## What is accepted

Suppose the proxy SSH name is `wt-git-build` and `github.com` is configured as:

```toml
[[providers]]
host = "github.com"
user = "git"
port = 22
private_key_file = "/etc/wt-git-proxy/github_ed25519"
known_hosts_file = "/etc/wt-git-proxy/github_known_hosts"
```

The client runs:

```console
git clone wt-git-build:github.com/team/project.git
```

Git asks OpenSSH to run:

```text
git-upload-pack 'github.com/team/project.git'
```

The proxy accepts that command and resolves it as:

- service: `git-upload-pack`;
- provider: the configured `github.com` entry;
- upstream repository: `team/project.git`.

It then connects to `git@github.com` on port 22 with the configured private key
and pinned `known_hosts` file, and asks the provider for:

```text
git-upload-pack team/project.git
```

A later push follows the same path, but Git requests:

```text
git-receive-pack 'github.com/team/project.git'
```

Only upload-pack and receive-pack are accepted. Repository paths must end in
`.git` and may contain letters, digits, `/`, `.`, `_`, and `-`. Empty
components, `.`, `..`, shell syntax, and unconfigured providers are rejected.

## When the core validates a push

The core starts the upstream Git service and forwards its branch advertisement
to the client. The client then sends the complete list of refs it wants to
update.

The core pauses there. It reads every ref in that list and checks it against the
write policy:

- If every ref is allowed, the core forwards the list and packfile upstream.
- If one ref is denied, the core forwards neither. It rejects the whole push
  locally and stops the upstream process.

The core does not apply the write policy to fetch, clone, or pull. The proxy has
already validated the service, provider, and repository path before the core
starts handling Git traffic.

## Client administration

The TUI adds, lists, and removes client keys. It can also generate an Ed25519
key bundle with a pinned proxy host key and ready-to-include SSH config.

Updating a client rewrites the managed `authorized_keys` file. Removing a key
blocks the next connection made with that key; it does not stop a connection
that is already running.
