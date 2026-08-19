# Risks and maintenance guide

This crate is small, but mistakes here can expose a repository or leave an
upstream Git process stuck. Pay attention in these places.

## Targets are already trusted

`GitTarget` does not make a path safe. A local absolute path or `..` can escape
the repository root. An unsafe SSH path can become part of a remote shell
command. Both frontends validate paths before they call this crate; keep that
true.

SSH host, user, key paths, and `known_hosts` paths must also come from trusted
configuration.

## Validate the whole push first

Do not forward ref commands as they are parsed. The crate must see and approve
every ref before the upstream receives any of them. Test a push containing one
allowed branch and one denied branch.

Prefix matching is textual. Keep the trailing slash and exact branch handling
deliberate. The constructor currently accepts repeated trailing slashes; the
frontends pass one.

## Clean up every process

The bridge uses a child process, two data directions, and a stderr pipe. Check
every error path after `spawn_git`: the child must be stopped and reaped, pipes
must keep draining, and threads must be joined.

There are two current rough edges worth remembering:

- errors after a good advertisement but before bridging do not share one child
  cleanup guard;
- `serve_git` starts draining stderr after the advertisement, so a very noisy
  upstream could block early.

## Keep parsing narrow and bounded

Pkt-line lengths and command fields come from the client. Keep the size limits
and bounds checks. Test truncated, oversized, malformed, and multi-ref pushes
when parsing changes.

This is not a general Git protocol library. Supporting a new capability needs
an end-to-end test, not just a more permissive parser.

## Watch memory when adding messages

Normal Git traffic streams. Adding a sideband push message is different: it
buffers the full upstream response, with no response-size limit today. The
message must use upstream `ok` results and must not claim every requested ref
succeeded.

## Before finishing

Run the crate's format, test, and Clippy checks. If a public behavior changes,
run both frontend test suites too. Use the real two-hop SSH test only for
changes that depend on OpenSSH.
