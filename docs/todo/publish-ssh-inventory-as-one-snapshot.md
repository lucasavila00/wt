# Publish the SSH inventory as one snapshot

The WT client atomically replaces `~/.ssh/wt/config` and
`~/.ssh/wt/known_hosts` independently. Concurrent `wt sync`, `wt ssh`, or
`wt shell` calls can interleave the two replacements and leave config from one
inventory with host keys from another.

The mixed snapshot can cause strict host-key failures or make aliases refer to
stale keys even though neither individual file is partial.

Serialize publication of both files with a cross-process lock for the managed
SSH directory. Add a concurrent test using disjoint inventories and verify that
the two published files always describe the same inventory generation.

Relevant code: `crates/products/wt/client/src/ssh.rs`.

Status: not addressed by the Codex session metadata work.
