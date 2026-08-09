# ADR 0017 implementation plan

This plan implements [ADR 0017](../adr/0017-integrate-agent-git-gateway.md)
in three separately merged milestones:

1. [Git transport and world integration](0017-agent-git-gateway/01-git-transport.md)
2. [GitHub API support](0017-agent-git-gateway/02-github-api.md)
3. [GitLab API support](0017-agent-git-gateway/03-gitlab-api.md)

Milestone 1 fixes the complete `ag-git` command and help contract. The docs and
help describe all operations immediately, even while provider commands still
report that their API implementation is unavailable.

## Clean installs only

This work does not preserve worlds, gateway state, or old configuration. Run
`make clear` or `make nuke` as required before reinstalling. Do not add database
migrations, configuration compatibility, or upgrade paths for the pre-ADR
system.

`ag-git`, `git-remote-ag`, the guest relay, and the gateway are always built and
installed. Every world uses them. There is no feature flag, legacy Git path, or
opt-out.

`ag-git` is a small POSIX shell frontend. All commands, help, output, policy,
and provider behavior live in the gateway. `git-remote-ag` and the guest relay
only transport requests and streams. Host updates apply to running worlds
without rebuilding them. Only a transport protocol change requires a clean
world rebuild.

## Provider configuration

GitHub and GitLab are independently optional. A normal installation must
configure at least one. `wt new` selects the provider from the repository host
and fails before creating a world when that provider is not configured.

Each provider block names an API-token file and an SSH key pair. TOML contains
paths, never a token or private-key value. The checked-in development config
enables GitHub only:

```toml
[agent_git.github]
host = "github.com"
api_token_file = "~/.config/wt/credentials/github.token"
ssh_private_key_file = "~/.ssh/id_ed25519"
ssh_public_key_file = "~/.ssh/id_ed25519.pub"
```

`DEVELOPMENT.md` tells the developer to create the token file outside the
checkout with mode `0600` before running:

```bash
scripts/install-server --config examples/server-config/wt-server.development.toml
```

The installer opens the token file without following symlinks. It requires a
nonempty regular file owned by the installing user with mode `0600`. It applies
the same ownership and no-group-or-other-access checks to the private key,
unlocks it when necessary, and proves that the public key matches.

The installer passes the token and unlocked key to the gateway as encrypted
systemd credentials. It never puts them in command arguments, environment
variables, or `/etc/wt/server.toml`.

Before creating a world, the gateway proves that the configured SSH key can read
the requested repository. GitHub API identity and write-permission checks land
in milestone 2; GitLab checks land in milestone 3.

## Common merge gate

Each milestone includes its schema, installer, clean-install instructions,
tests, and user-visible text. Run formatting, tests, and Clippy for every
affected Rust crate.

No automated test uses provider credentials or contacts GitHub or GitLab. Real
provider behavior is covered by human release QA against dedicated test
projects, with credentials held only by the installed gateway.
