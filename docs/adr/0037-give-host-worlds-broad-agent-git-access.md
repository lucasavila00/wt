# ADR 0037: Give every world broad agent Git access

- Status: Accepted
- Date: 2026-08-16
- Extends: [ADR 0017](0017-integrate-agent-git-gateway.md)
- Amends: [ADR 0017](0017-integrate-agent-git-gateway.md)'s grant scope

## Context

Gateway grants are tied to one repository. This prevents host worlds from using
`ag-git` and prevents devcontainer worlds from working across repositories.

## Decision

Use the same grant rules for every world. A grant is bound to the world, not a
repository. It can read every repository available to the gateway's configured
provider credentials. It can write only branches under `wt/`; reject other
branches and tags. Apply the same rule to `ag-git` mutations by resolving the
named MR or CI resource before changing it.

A world may open an MR from an existing `wt/*` branch to any explicit base in
the same repository. Looking up an MR by branch must find exactly one open MR.

No per-repository gateway setup is required. World setup configures Git to route
normal SSH and HTTPS URLs for each configured provider through `ag::`. Existing
origins and ordinary clone, fetch, pull, and push commands use the gateway.

`ag-git` reads the current checkout's `origin` to tell the gateway which
repository an explicit command is about. The caller does not pass or configure
it. Help and reporting still work outside a checkout.

Install the relay, `git-remote-ag`, and `ag-git` in every new host world. Persist
each world's grant, start its relay during provisioning and on boot, and revoke
it when the world is removed.

## Consequences

- Every world can use Git and `ag-git` across repositories without receiving
  provider credentials.
- World write access remains limited to `wt/*` branches.
- Existing worlds must be recreated to use the repository-independent client
  and grant.
