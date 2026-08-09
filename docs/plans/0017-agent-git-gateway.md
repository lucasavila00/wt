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
installed. There is no feature flag.

## Provider configuration

GitHub and GitLab are independently optional. A normal installation must
configure at least one. `wt new` selects the provider from the repository host
and fails before creating a world when that provider is not configured.

Each provider block names an API-token environment variable and an SSH key
pair. TOML contains paths and the environment-variable name, never a token or
private-key value. The checked-in development config enables GitHub only:

```toml
[agent_git.github]
host = "github.com"
api_token_env = "GITHUB_TOKEN"
ssh_private_key_file = "~/.ssh/id_ed25519"
ssh_public_key_file = "~/.ssh/id_ed25519.pub"
```

`DEVELOPMENT.md` requires `GITHUB_TOKEN` in the developer's `.bashrc` before
running:

```bash
scripts/install-server --config examples/server-config/wt-server.development.toml
```

The installer fails when the configured environment variable is empty. It
expands the key paths, checks their ownership and permissions, unlocks the
private key when necessary, and proves that the public and private keys match.
It installs the API token and unlocked key as encrypted systemd credentials for
the gateway. Neither secret is written to `/etc/wt/server.toml`.

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
