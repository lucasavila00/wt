# ADR 0036: Sync managed SSH before connecting

- Status: Accepted
- Date: 2026-08-16

## Context

Users normally enter a retained world through its managed OpenSSH alias. A
world can be created, removed, or started from another workstation, so the
local alias inventory may be stale when a user runs `ssh NAME`. The safe manual
sequence is `wt sync` followed by `ssh CONTEXT.NAME`, but it requires two
commands and makes the user resolve short names themselves.

`wt code NAME` already accepts a short or qualified world name, requires the
complete cross-context inventory, synchronizes managed SSH configuration, and
then opens the qualified editor alias.

## Decision

Add `wt ssh NAME` for retained devcontainer and host worlds with a managed SSH
alias. It:

1. reads the complete world inventory and resolves `NAME` using the same
   unique-short-name rules as other client commands;
2. rejects worlds whose lifecycle state has no managed SSH alias;
3. synchronizes the complete managed SSH inventory; and
4. replaces the `wt` process with `ssh -- CONTEXT.NAME`.

Always use the qualified alias after resolution. This keeps the selected target
stable even when the same short world name exists in another context later.
Do not retain `wt` as a wrapper: OpenSSH must own the terminal, signals, and
exit status. If inventory collection or synchronization fails, do not start
OpenSSH.

This command opens the regular persistent Byobu alias. Direct guest and editor
aliases remain available through OpenSSH as `CONTEXT.NAME-host` and
`CONTEXT.NAME-vs` where the world kind provides them.

## Verification

- Verify short and qualified targets exec OpenSSH with only `--` and the
  qualified regular alias.
- Verify the managed inventory exists before OpenSSH starts.
- Verify synchronization failure does not start OpenSSH.
- Verify OpenSSH's exit status becomes the `wt ssh` exit status.

## Consequences

- A single command refreshes local host keys and connects to a world.
- Ambiguous short names fail before synchronization or connection and list the
  qualified choices.
- `wt ssh` cannot connect when any context is unavailable because rewriting the
  inventory from a partial list could discard valid aliases.
- Users still use OpenSSH directly for `-host` and `-vs` access.

## Alternatives

### Run `wt sync && ssh NAME` manually

Rejected because it does not resolve ambiguity to a stable qualified target and
repeats a routine safety step at every connection.

### Spawn OpenSSH and wait

Rejected because `wt` would become an idle process wrapper and would need to
duplicate OpenSSH lifecycle handling.
