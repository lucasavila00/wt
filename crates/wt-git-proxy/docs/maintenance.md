# Risks and where to pay attention

The proxy holds an upstream private key. Treat command parsing, generated
`authorized_keys` entries, and config files as security-sensitive code.

## Keep the two credentials separate

Client keys only open the proxy account. The configured provider key only opens
the upstream repository host. Do not copy the provider key into a client bundle
or pass it through the Git stream.

Keep strict host-key checking, `BatchMode=yes`, and `IdentitiesOnly=yes` on the
upstream connection. Generated client bundles also pin the proxy host key.

## Know the access scope

Every authorized client can try any repository on a configured provider. What
it can read is limited by the upstream key, not by a repository allowlist in
the proxy. Every client also shares the same write policy. Use a dedicated
upstream key whose provider permissions match that scope.

## Keep the forced command narrow

Accept only upload-pack, receive-pack, a configured provider, and a safe `.git`
path. A small parser here is intentional. Do not pass `SSH_ORIGINAL_COMMAND` to
a shell or make it accept general SSH commands.

The managed `authorized_keys` lines use `restrict` and a forced command. Changes
to their quoting or path validation need tests with hostile spaces, quotes,
shell characters, absolute paths, and `..`.

## Protect managed files

The config contains provider key paths and the generated client bundle contains
a private key. The proxy writes config and `authorized_keys` with mode `0600`
and their directories with mode `0700`. Preserve those modes.

Managed files are replaced atomically through a `.wt-new` file. A stale
temporary file makes the next save fail instead of overwriting unknown data.

## Remember what revocation means

Removing a client key stops new SSH connections. It does not kill a session
that is already connected. There are no tokens, expiries, or background
revocation service.

## Test both SSH hops

Unit tests cover config and key management. Changes to forced commands, key
selection, host-key checking, or stdio transport need the two-hop OpenSSH
integration test in `wt-integration-tests`.

The packet bridge and branch decision live in `wt-git-core`. Keep this crate
focused on OpenSSH setup, safe command resolution, config, and client keys.
