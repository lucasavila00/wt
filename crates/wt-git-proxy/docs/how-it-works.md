# How it works

There are two SSH connections and two different keys:

```text
agent -- client key --> proxy -- provider key --> GitHub
```

The agent never receives the provider key. GitHub never receives the agent key.

## An existing clone

Suppose an agent already has this origin:

```text
https://github.com/team/project.git
```

The command printed by the dashboard installs this Git rewrite:

```gitconfig
[url "wt-git-abc123:github.com/"]
    insteadOf = https://github.com/
    insteadOf = git@github.com:
    insteadOf = ssh://git@github.com/
```

Git keeps the stored origin unchanged, but connects to:

```text
wt-git-abc123:github.com/team/project.git
```

OpenSSH uses the generated client key. The proxy account's `authorized_keys`
entry forces `wt-git-proxy serve`, so that key cannot open a shell.

## What the proxy checks

Git asks the proxy to run:

```text
git-upload-pack 'github.com/team/project.git'
```

The proxy checks this immediately, before opening the provider connection:

- the service is exactly `git-upload-pack` or `git-receive-pack`;
- `github.com` is configured;
- `team/project.git` is a safe repository path.

It then connects to `git@github.com` on port 22 with the installed provider
key and pinned provider host key. Provider user and port are fixed setup
defaults, not dashboard questions.

## When the core checks a push

The repository path is known when the connection opens, but the refs are not.
The core therefore checks branches later:

1. The proxy validates the service, provider, and repository path.
2. The core starts the provider's `git-receive-pack`.
3. The core forwards the provider's ref advertisement to the agent.
4. The agent sends the complete list of refs it wants to update.
5. The core pauses the push and checks every ref.
6. If every ref is allowed, it forwards the commands and packfile.
7. If one ref is denied, it forwards neither and rejects the whole push.

The core does not apply branch policy to clone, fetch, or pull. The provider
still applies its own repository permissions and branch protection after the
core permits a push.
