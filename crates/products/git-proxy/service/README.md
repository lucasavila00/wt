# wt-git-proxy

`wt-git-proxy` lets agents use Git without giving them a Git provider's private
key. Agents get their own proxy key instead. The proxy keeps provider keys and
only allows pushes to the branches you choose.

There is no proxy daemon. OpenSSH starts one short-lived process for each Git
connection.

## Install

The checked-in example is:

```toml
version = 1
client_port = 22
write_prefix = "agents/"
allowed_branches = ["main"]

[[providers]]
host = "github.com"
private_key_file = "~/.ssh/id_ed25519"
known_hosts_file = "~/.ssh/known_hosts"
```

The provider key must already exist. Setup never generates or changes it. The
private key must be owned by the installing user with mode `0600`. The pinned
`known_hosts` file may use `0600` or `0644`.

Install from a WT checkout on Ubuntu 24.04 amd64:

```console
scripts/install-git-server --config examples/git-proxy-config/wt-git-proxy.development.toml
```

Setup uses the same credential code as regular WT setup. If the provider key is
encrypted, it asks for its passphrase and installs an unlocked `0600` copy for
the `git-proxy` account. The original key is not changed.

The installer finishes at a small client dashboard:

```text
WT Git proxy

SPACE  Generate and authorize an agent key
R      Revoke an agent key
Q      Quit
```

During setup, the installer looks up the server's current public IPv4 address
and suggests `git-proxy@<address>`. Confirm it or enter a reachable IP or DNS
name. The lookup is only a suggestion because NAT may use a different inbound
address. If the lookup fails, enter the address manually. The suggestion comes
from `https://api.ipify.org`; setup does not send credentials to it.

The client SSH port is file-only: `client_port` defaults to 22 and is never
prompted for. The dashboard shows the confirmed destination and configured
port embedded in generated commands.

Configuration is changed in the TOML file, not in the dashboard. Rerun the same
install command after changing it.

## Give an agent access

Press Space. The dashboard prints a readable report showing the server change,
every file that will be written on the agent, and changes to the agent's Git and
SSH configuration. The report includes the private key in plain text, so treat
the whole report as secret.

The report also prints one shell command. Paste that command into the agent VM
as the user that runs Git.

The command:

- installs the client key and pinned proxy host key under
  `~/.ssh/wt-git-proxy/<client>/`;
- adds one include to `~/.ssh/config`;
- adds global Git URL rewrites for every configured provider.

Repositories may already be cloned. An existing origin such as
`https://github.com/team/project.git` or `git@github.com:team/project.git`
will use the proxy on its next fetch or push. The stored origin is not changed.
Pasting a later generated command replaces the managed rewrite, so rotating a
revoked client does not require editing repositories.

With the example policy, agents may push `agents/fix-login` and `main`.
They may not push `feature/fix-login` or tags. Clone, fetch, and pull are not
restricted by the branch policy.

Press R to revoke a client. Revocation blocks its next SSH connection but does
not stop a connection already in progress.

## Roll back an agent

Each client report includes a rollback command. It removes the Git and SSH
includes and deletes `~/.ssh/wt-git-proxy` for that agent user. Repository
remotes were never changed, so they immediately use their original provider
URLs again.

Client cleanup does not revoke the key on the server. Press R in the dashboard
and select the same client to remove its server access.

## Important risks

- The printed report and command contain a private key. Do not put them in logs,
  tickets, images, or shared shell history.
- Every client can read every repository readable by the provider key. The
  proxy does not have a repository allowlist.
- The provider still decides whether a push is allowed. Its permissions and
  branch rules apply after the proxy policy.
- A repository with an explicit noncanonical `remote.*.pushurl` may bypass the
  global URL rewrite. Remove it or point it at a canonical provider URL.

## More detail

- [How a connection moves through the proxy](docs/how-it-works.md)
- [How the write policy works](docs/write-policy.md)
- [Where to pay attention when changing it](docs/maintenance.md)
