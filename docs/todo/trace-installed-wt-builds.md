# Trace installed WT builds to source commits

An installed `wt` client and `wt-server` do not expose the source revision they
were built from. `wt --version` is not currently available, and the server's
service status and startup log identify only the executable path. When client,
server, image, and guest behavior disagree, operators must compare timestamps
or binary hashes and still cannot reliably identify the source tree.

Define one build identity shared by the client, server, installer, and retained
image provenance. At minimum it should contain the package version and full Git
commit SHA. Make it easy to retrieve through `wt --version`, a server command,
and the server startup journal. Return both client and server identities in a
diagnostic command so remote-context mismatches are visible in one place.

Production installation and release entry points must reject a source tree with
tracked staged or unstaged changes before building `wt` or `wt-server`. This
check belongs at the release/install boundary so normal debug edit-test loops
remain possible. Decide explicitly whether untracked files that can affect a
build are rejected or included in a reproducible source-content digest.

Tests should prove that:

- clean committed sources produce the same identity in client, server, and
  image provenance;
- staged and unstaged tracked changes stop production builds with an actionable
  error that names the dirty paths;
- `wt --version`, the server diagnostic, and startup logs render the complete
  identity;
- a client/server identity mismatch is reported without relying only on the
  control-protocol version.
