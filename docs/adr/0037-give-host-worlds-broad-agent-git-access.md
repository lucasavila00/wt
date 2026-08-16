# ADR 0037: Give every world broad agent Git access

- Status: Accepted
- Date: 2026-08-16
- Extends: [ADR 0017](0017-integrate-agent-git-gateway.md)
- Amends: [ADR 0017](0017-integrate-agent-git-gateway.md)'s grant scope
- Amended by: [ADR 0040](0040-stop-automatic-ssh-agent-forwarding.md)

## Context

Gateway grants are tied to one repository. This prevents host worlds from using
`ag-git` and prevents devcontainer worlds from working across repositories.

## Decision

Use the same grant rules for every world. A grant is bound to the world, not a
repository. Through the gateway it can read every repository available to the
configured provider credentials and write only branches under `wt/`; reject
other branches and tags. Apply the same rule to `ag-git` mutations by resolving
the named MR, thread, or CI resource before changing it.

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

Host SSH agent forwarding remains a separate, unrestricted credential path.
The `wt/*` rule governs gateway traffic, not direct use of the forwarded agent.
Revoke old repository-scoped grants when the gateway loads them; those worlds
must be recreated.

## Consequences

- Every world can use the gateway across repositories without receiving its
  provider keys or tokens.
- Gateway write access remains limited to `wt/*` branches.
- Existing worlds must be recreated after their old grants are revoked.
