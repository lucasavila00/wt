# Make Codex auth sharing convergent

The Codex auth sharing helper stages through a fixed path, publishes a hard
link, and then checks whether the source inode changed. If Codex atomically
replaces `auth.json` during that sequence, the helper can publish the older
inode and fail. The path event may be coalesced while the oneshot service is
active, leaving retained worlds with stale credentials until another change.

Overlapping manual or installer invocations can also remove each other's fixed
staging link.

Serialize helper invocations and use a unique staging path. Retry the
link-and-publish operation until the source inode is stable. Add tests that
replace the source at each publication checkpoint and require the final shared
inode to match the final source.

Relevant code:

- `assets/server/share-codex-auth.sh`
- `crates/products/wt/server-installer/src/server.rs`
