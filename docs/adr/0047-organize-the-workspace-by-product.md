# ADR 0047: Organize the workspace by product

- Status: Accepted
- Date: 2026-08-21
- Amends: [ADR 0026](0026-make-world-kinds-first-class.md),
  [ADR 0041](0041-publish-a-standalone-git-proxy.md)

## Context

The flat workspace reflects how WT grew from devcontainers into host worlds,
GitHub runners, agent Git, and a standalone Git proxy. Crate names and ownership
are now unclear, and retained-world types and traits are duplicated across
several crates.

WT is at version `0.0.0`. We can use `make nuke`, recreate worlds, and rely on
E2E tests instead of preserving the current package layout or installed state.

## Decision

Organize the workspace by product:

```text
crates/products/wt/{client,control-protocol,server,retained-worlds}
crates/products/wt/{devcontainer-guest-tools,codex-integration,server-installer}
crates/products/gh-actions-runner/
crates/products/agent-git-gateway/{gateway,git-hosting}
crates/products/git-proxy/{service,installer}
crates/shared/{libvirt-kvm,workload-registry,git-smart-protocol,installer-support}
crates/tests/end-to-end/
```

Use this crate map:

| Current | Target |
|---|---|
| `wt-api` | `wt-control-protocol` |
| `wt-cli` | `wt-client` |
| `wt-server` | keep |
| `wt-devcontainer` + `wt-host` + `wt-retained` + `wt-server::worlds` | `wt-retained-worlds` |
| `wt-provider` + `wt-libvirt` | `wt-libvirt-kvm` |
| `wt-registry` + `wt-server::store` | `wt-workload-registry` |
| `wt-github-ci` | `wt-gh-actions-runner` |
| provider API and CLI code from `wt-agent-git` | `wt-git-hosting` |
| remaining `wt-agent-git` | `wt-agent-git-gateway` |
| `wt-git-core` | `wt-git-smart-protocol` |
| `wt-git-proxy` | keep |
| `wt-devcontainer-guest` | `wt-devcontainer-guest-tools` |
| `wt-command` | delete and use `std::process::Command` directly |
| `wt-setup-core` | `wt-installer-support` |
| `wt-server-setup` | `wt-server-installer` |
| `wt-git-proxy-setup` | `wt-git-proxy-installer` |
| `wt-codex` | `wt-codex-integration` |
| `wt-integration-tests` | `wt-end-to-end-tests` |

Ownership rules:

- `wt-retained-worlds` owns devcontainer and host lifecycle.
  `wt-workload-registry` alone owns SQLite, migrations, capacity, and persisted
  records. `wt-gh-actions-runner` owns ephemeral GitHub Actions runners.
- `wt-libvirt-kvm` owns the supported libvirt/KVM implementation. Do not maintain a
  separate provider abstraction until a second production provider exists.
- `wt-agent-git-gateway` owns world-to-gateway transport; `wt-git-hosting` owns
  GitHub and GitLab operations; `wt-git-smart-protocol` is shared with the
  standalone proxy.
- `wt-server` composes features but does not absorb their implementation.
  Host and image setup remains in `wt-server-setup`.
- Production code never depends on installers or `wt-end-to-end-tests`. A new
  shared crate must have two production consumers from different products.

Rename installed binaries with their crates:

| Current | Target |
|---|---|
| `wt-server-setup` | `wt-server-installer` |
| `wt-git-proxy-setup` | `wt-git-proxy-installer` |
| `wt-codex` | `wt-codex-integration` |
| `ag-git` | `wt-git-hosting` |
| `git-remote-ag` | `git-remote-wt-agent` |
| `wt-app-pane` | `wt-devcontainer-pane` |
| `wt-app-proxy` | `wt-devcontainer-ssh-proxy` |
| `wt-app-info` | `wt-devcontainer-info` |
| `wt-agent-git-relay` | keep |
| `wt-agent-git-gateway` | keep |

The future GitHub Actions operator binary is `wt-gh-actions-runner`.

This is a clean break. Extend `make nuke` to remove old worlds, images, state,
SSH configuration, services, and installed helpers. Do not add compatibility
aliases or data migrations.

Move one product slice at a time. Do not combine mechanical moves with behavior
changes. After the final move, run the fresh-install E2E flows for devcontainer,
host, local/OpenSSH clients, guest SSH, agent Git, and the standalone Git proxy,
then verify `make nuke` removes the installation.

## Consequences

- The tree, crate names, and binary names show which product owns each feature.
- Retained worlds have one lifecycle crate and KVM has one implementation crate.
- Existing development installations and worlds must be recreated.
