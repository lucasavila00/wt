# Write policy and push validation

The policy has two parts:

- one branch prefix, such as `agents/`;
- zero or more exact branches, such as `main`.

That example allows `agents/fix`, `agents/docs`, and `main`. It does not allow
`fix`, `other/fix`, or any tag.

The prefix itself is not a branch. Something must come after its final slash.

## The important guarantee

The crate checks every ref before it forwards the push. One denied ref rejects
the whole push, including refs that would have been allowed on their own.

Allowed branches may be created or updated only by fast-forward. History
rewrites (including `--force-with-lease`) and branch deletions are rejected by
the gateway regardless of upstream protection settings. Tags and every other
non-branch ref are always denied. A force flag on an actual fast-forward does
not discard history and is allowed.

The gateway stages incoming objects and checks that each existing branch's old
commit is an ancestor of its new commit before forwarding any ref commands.
One invalid update rejects the entire push. Original old object IDs are passed
upstream so a concurrent change still causes a stale-ref rejection.

## Configuration validation

`WritePolicy` receives fully qualified refs. For the example above, they are
`refs/heads/agents/` and `refs/heads/main`.

Prefixes must start with `refs/heads/` and end with `/`. Exact branches must
start with `refs/heads/`. Names use simple ASCII branch components.

Frontends may use short names and add `refs/heads/` before building the policy.

This is not a full replacement for `git check-ref-format`. The upstream Git
service still makes the final call on whether an allowed ref name is valid.
