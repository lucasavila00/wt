# Architecture

```text
wt client
  └─ schema-versioned JSON session over local process or OpenSSH
      └─ wt-server API bridge: commands, terminal messages, client effects
          └─ typed requests over Unix socket (0600)
              └─ wt-server
                  ├─ shared registry and capacity
                  ├─ devcontainer lifecycle
                  └─ host lifecycle

wt-github-ci library foundation
  └─ GitHub CI lifecycle ─────── shared registry and capacity

devcontainer / host / github-ci
  └─ wt-provider contracts ──── wt-libvirt ── KVM + QEMU guest agent
```

`wt-server` owns retained worlds. The CI library uses the same registry
admission model; its operator process is not shipped yet.

The control plane has no TCP listener. The thin client selects one context and
runs a schema-versioned JSON session with `wt-server api`. The bridge owns the
command grammar and talks to the protected daemon socket through the internal
typed API. Client schema 1 carries ordered terminal messages and a closed enum
of workstation effects.

## Crates

| Scope | Crates |
|-------|--------|
| Shared | `wt-api`, `wt-cli`, `wt-command`, `wt-provider`, `wt-libvirt`, `wt-registry`, `wt-server`, `wt-server-setup`, `wt-integration-tests` |
| Devcontainer | `wt-devcontainer`, `wt-devcontainer-guest`, `wt-devcontainer-git` |
| Host | `wt-host` |
| GitHub CI | `wt-github-ci` |

Generic names are used only for behavior shared by more than one kind.
Executable names used inside existing guests remain stable.

## State

- Client contexts: `~/.wt/config.toml`
- Managed SSH: `~/.ssh/wt/`, partitioned by context
- Server configuration: `/etc/wt/server.toml`
- Capacity configuration: `/etc/wt/capacity.toml`
- Shared registry: `~/.local/state/wt/instances.db`
- KVM machine files: the configured libvirt worlds directories

Application data is typed by kind. A host record cannot acquire devcontainer
Git or app-SSH state, and a CI record cannot appear in retained-world inventory.
