# Serialize Git proxy authorized-key updates

Git proxy key additions and removals perform an unlocked read-modify-write of
`authorized_keys`. The final write also stages through the fixed
`authorized_keys.wt-new` path.

Concurrent admin sessions can either collide while creating the staging file or
silently overwrite an update based on stale input. In the worst case, a session
adding one key can restore a key that another session just revoked.

Hold a cross-process advisory lock across the complete read, validation, and
replacement transaction. Use a unique same-directory temporary file for the
atomic replacement. Add a multi-process regression test that interleaves an
addition and a revocation and verifies that the added key is present while the
revoked key remains absent.

Relevant code:

- `crates/products/git-proxy/service/src/admin.rs`
- `crates/products/git-proxy/service/src/config.rs`
