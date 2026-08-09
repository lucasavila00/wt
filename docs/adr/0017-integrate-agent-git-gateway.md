# ADR 0017: Give every world scoped Git access

- Status: Proposed
- Date: 2026-08-09
- Supersedes: [ADR 0001](0001-agent-forwarded-first-ssh-provisioning.md)'s
  Git authentication and
  [ADR 0003](0003-forward-the-workstation-ssh-agent-to-devcontainers.md)

## Context

WT currently forwards the developer's SSH agent into every world. That gives
code in the world the developer's Git access and makes safe automation harder.

Agents still need to push code, open a pull or merge request, respond to
reviews, and work through CI failures.

## Decision

Use the gateway for every world. Delete WT's SSH-agent forwarding code; there
is no legacy mode or opt-out. Generated SSH config does not set `ForwardAgent`,
world setup does not read `SSH_AUTH_SOCK`, and guests and devcontainers do not
receive a forwarded socket.

Build and release the gateway, guest relay, `git-remote-ag`, and `ag-git` in the
WT monorepo. `wt-server-setup` installs and manages every component. The systemd
service layout is internal to WT.

`ag-git` is a small transport client. The gateway owns commands, output, policy,
and provider integrations. Updating the host services fixes every existing
world without rebuilding it.

The guest relay and `git-remote-ag` are stable transport shims. Ordinary
gateway changes must not require changes inside a world. A transport protocol
change may require `make clear` or `make nuke` and a rebuild.

`wt new` requires a branch revision. That branch becomes the immutable base for
the world's pull or merge requests.

## Local transport

The gateway has no listening IP socket:

```text
wt-server ── Unix socket ──> agent Git gateway ──> GitHub or GitLab

devcontainer ── Unix socket ──> guest relay ── KVM vsock ──> gateway
   git-remote-ag / ag-git
```

The gateway is the only component that connects to GitHub or GitLab. It needs
outbound provider access.

The guest relay avoids giving the devcontainer direct vsock access. World setup
mounts the relay's Unix socket into the primary devcontainer, where
`git-remote-ag` and `ag-git` use it automatically.

The workstation never talks to the gateway. A Mac continues to use WT by
connecting to the Linux server and its worlds over SSH.

## Credential ownership

| Location | Credential for agent Git |
|----------|--------------------------|
| Workstation | None |
| `wt-server` | None; Unix socket permissions authorize it |
| Guest relay | One scoped gateway token |
| Primary devcontainer | Scoped relay socket; no key or token |
| Agent Git gateway | Dedicated GitHub or GitLab Git and API credentials |

During creation, `wt-server` asks the gateway for a token limited to the
selected project, base branch, and world name. World setup stores it on the
private disk, outside the checkout, and starts the relay.

Worlds never receive the developer's SSH keys or provider credentials.

The gateway is the central trust boundary. `wt-server-setup` configures provider
credentials once on the Linux host. If the gateway is unavailable, worlds can
keep working locally but cannot fetch, push, or use `ag-git` until it returns.

## SSH agent forwarding

ADR 0001 forwarded the developer's agent for the initial clone. ADR 0003 kept
forwarding it so later Git commands inside the devcontainer could use the same
SSH identities. Because `ssh NAME` crosses the guest before entering the
devcontainer, WT had to relay `SSH_AUTH_SOCK` across that second hop and retarget
the relay after every reconnect.

The gateway now authenticates both the initial clone and later Git operations.
No WT workflow needs the developer's agent, so WT removes the forwarding config,
socket relay, reconnect handling, and devcontainer handoff.

WT does not disable OpenSSH's native forwarding. A developer can still choose
to expose their agent for one connection:

- `ssh -A NAME-host` exposes it in the guest.
- `ssh -A NAME-vs` exposes it directly in the devcontainer.
- `ssh -A NAME` exposes it in the guest, but WT does not carry it through
  `wt-app-pane` into the devcontainer.

These connections bypass WT's credential isolation and are the developer's
responsibility. SSH port forwarding with `-L`, `-R`, or `-D` is unrelated and
continues to work normally.

## Git workflow

The world name is the required branch prefix. In a world named `df1`:

```text
git switch -c df1/fix-login
# edit and commit
git push
```

WT configures `origin` with an `ag::` URL and enables automatic upstream setup.
Normal `git push`, `git fetch`, and `git pull` invoke `git-remote-ag` and need no
gateway URL or extra command.

The gateway pushes branches unchanged and accepts only the world's prefix.

The token allows updates, force-pushes, and deletions under `df1/`. The gateway
rejects other branches and tags. The `df1/` namespace is reserved for this
project and world. Creation fails if the prefix already exists unless an earlier
instance of the same project and world owns it.

GitHub or GitLab is the durable Git store. Gateway caches are disposable.

## Discoverability

WT installs `post-checkout` and `post-commit` hints without replacing project
hooks. They never change or reject a Git operation and never contact the
gateway. Repeating a useful hint is fine.

Every hint assumes the reader knows nothing about WT or `ag-git` and starts with
this complete header:

```text
WT: This is a WT-managed development environment for a coding agent.
WT: For safety, the developer's SSH keys and GitHub or GitLab credentials are
WT: not available here. Do not look for credentials or use `gh` or `glab`.
WT: WT gives you scoped access to project `group/project`.
WT: Use normal Git for commits, fetches, pulls, and pushes. Every branch you
WT: push must start with `df1/`. Pull or merge requests target `main`.
WT: `ag-git` is the installed CLI for pull or merge requests, reviews, and CI.
WT: Run `ag-git` for the current branch's status and suggested next actions.
WT: Run `ag-git --help` to discover every available command.
WT:
```

Git renders the same gateway header with `remote:` in place of `WT:`.

Checking out an invalid branch explains the rule and the fix:

```text
WT: This world can only push branches that start with `df1/`.
WT: Rename the current branch before pushing:
WT:   git branch -m df1/fix-login
```

After a commit, WT explains the whole next step:

```text
WT: Commit created on `df1/fix-login`.
WT: Publish it with:
WT:   git push
WT: After pushing, run `ag-git` to open or manage its pull or merge request.
```

The gateway repeats the branch-name guidance if a bad branch reaches it. After
a successful push without a request, it prints:

```text
remote: Published branch `df1/fix-login`.
remote: This branch does not have a pull or merge request.
remote: Open one for review:
remote:   ag-git open-mr
remote: Or open it as a draft:
remote:   ag-git open-mr --draft
```

If a request already exists, the push output says what changed:

```text
remote: Updated merge request !123: https://gitlab.example/project/-/merge_requests/123
remote: Run `ag-git` to see review comments and CI.
```

## Pull and merge requests

Pushing only publishes the branch. `ag-git open-mr` opens a ready request
against the world's base; `--draft` opens a draft. If a request already exists,
the command shows it.

The gateway owns the provider workflow exposed through `ag-git`: opening or
showing the request, addressing reviews, investigating or controlling CI, and
waiting for review or CI changes. Its output uses short handles and shows the
relevant next commands.

The gateway enforces branch, request, review, and CI scope. The agent can prepare
its request for human merge, but cannot merge or approve it, change its base,
dismiss reviews, or act outside its namespace. CI commands apply only to the
current commit. GitHub or GitLab remains authoritative for repository
permissions and protections.

Comments use the gateway's provider identity and include the world name.

## Lifecycle

`wt rm` revokes the world's token but leaves external branches and requests
intact. Recreating the same world for the same project gets a new token and
continues the existing namespace.

`wt fork` is unavailable because it would copy a world's token and namespace.
