# ADR 0034: Use a thin schema-versioned client

- Status: Accepted
- Date: 2026-08-15
- Supersedes: [ADR 0007](0007-require-matching-client-and-server-commits.md)
- Amends: [ADR 0011](0011-exec-ssh-after-world-creation.md) and
  [ADR 0033](0033-bootstrap-managed-ssh-configuration.md)

## Decision

`wt` will be a small interpreter for a versioned protocol. `wt-server` will own
the command grammar, validation, interactive flow, formatting, and lifecycle
decisions.

The client will:

- read `~/.wt/config.toml`;
- select exactly one context;
- remove the global `--ctx NAME` argument;
- send the remaining UTF-8 arguments unchanged;
- forward standard input and print server output; and
- execute a closed enum of workstation effects.

With one configured context, `--ctx` is optional. With two or more, every
server command requires it. A command never queries more than one server.
World arguments are unqualified names on the selected server.

The existing local and OpenSSH helpers will carry a framed JSON conversation:

```text
client -> server: Start { schema, context, args }
client -> server: Input { id, text | eof }
client -> server: EffectResult { id, result }

server -> client: Ready { schema }
server -> client: Output { stdout | stderr, text }
server -> client: ReadInput { id }
server -> client: Effect { id, effect }
server -> client: Exit { code }
```

The first `ClientEffect` variants will cover:

- reading Git identity and SSH public keys;
- replacing the selected context's managed SSH inventory;
- launching VS Code; and
- `exec` of OpenSSH for a login transition.

There will be no generic command, shell, or arbitrary file-write effect.
`exec_ssh` is terminal and must run after earlier output and SSH inventory
effects are complete.

SSH inventory will be stored per context. Updating one context will not contact
or rewrite another. Only context-qualified SSH aliases will be generated.

The client and server must have the same schema, not the same Git commit. The
server will reject a schema mismatch before executing the command or changing
state. The error will show both schemas and require a client upgrade.

Keep the schema unchanged for new commands and changes to server behavior,
prompts, validation, diagnostics, and output. Bump it when a message changes
incompatibly or the client needs a new effect. The server will support one
schema, with no backward-compatibility modes.

Workstation file input will use standard input. In particular, host user-data
will no longer be opened by path after dispatch to a remote server.

Server upgrade delivery remains in `wt-server-setup`.

## Verification

- Different commits with the same schema work together.
- A schema mismatch fails before a mutating command reaches the service.
- A new server-only command works without rebuilding the client.
- Two configured contexts require `--ctx` and start only the selected helper.
- Input, output, effect results, and exit codes work locally and over OpenSSH.
- Unknown effects fail without executing local work.
- SSH inventory updates only the selected context.
