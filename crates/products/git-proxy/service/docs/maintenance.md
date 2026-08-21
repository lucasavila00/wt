# Risks and maintenance

This code sits between an untrusted Git client and a powerful provider key.
Keep its boundaries small.

## Credentials

The client key only opens the proxy account. The provider key only opens the
provider. Never put the provider key in a generated client command.

Setup shares its file checks, key parsing, passphrase handling, and secure
installation code with `wt-server-installer` through `wt-installer-support`. Changes
there affect both installers and need tests for both.

First-time setup asks `api.ipify.org` for a public IPv4 suggestion. Treat it as
a convenience, not proof that the address accepts inbound SSH through NAT or a
firewall. The operator-confirmed address is stored in the runtime config.

Keep strict host-key checking, `BatchMode=yes`, and `IdentitiesOnly=yes` on
the provider connection. Client commands must keep pinning the proxy host key.

## Access scope

The branch policy is global. So is repository access: every client can try
every repository readable by the provider key. Use a dedicated provider key
whose repository permissions match the agents that receive client commands.

Global Git `insteadOf` rules cover normal HTTPS and SSH origins for every
configured provider. They do not guarantee interception of a custom
`remote.*.pushurl` or an unusual provider URL. Pay attention to those when
auditing an existing VM.

## Command parsing

The proxy accepts only upload-pack, receive-pack, a configured provider, and a
safe path ending in `.git`. Never pass `SSH_ORIGINAL_COMMAND` to a shell.

Managed `authorized_keys` entries use `restrict` and a forced command.
Changes to their quoting or path validation need hostile-input tests.

## Revocation

Removing a key blocks new SSH connections. It does not kill a connection that
is already running. There are no expiries or background revocation service.

## Testing

Unit tests cover config, credential installation, client commands, and key
management. Changes to either SSH hop or stdio transport also need the two-hop
OpenSSH integration test in `wt-end-to-end-tests`.

The packet bridge and branch decision belong in `wt-git-smart-protocol`. Keep this crate
focused on OpenSSH, safe command resolution, config, and client access.
