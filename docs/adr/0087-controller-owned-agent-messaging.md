# ADR 0087: Controller-owned agent messaging

- Status: Accepted
- Date: 2026-09-05

## Decision

WT provides worlds and bounded generic command execution. Controllers own agent
runtime installation, supervision, explicit parent messages, and terminal results.
Apr uses agapi's single durable outbox for both explicit messages and Codex results.
It imports events transactionally before acknowledging them.

WT has no parent mailbox. Remove `wt messages`, `read_world_mail`, the `wtg tools`
parent-message action, terminal-result transport messages, and mailbox registry tables.
There is no compatibility bridge. Git-provider tools, tool feedback, terminal-pane
observations, and Codex authentication/session mounts are unchanged.

## Consequences

Controller runtimes can use the same message protocol on WT worlds and disposable
test containers. Agent message delivery no longer depends on WT's guest gateway,
vsock identity, or registry. WT never needs to interpret an agent thread or turn.
