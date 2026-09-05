# Write policy

The proxy has one policy for every client and provider:

```toml
write_prefix = "agents/"
allowed_branches = ["main"]
```

This allows any branch below `agents/`, plus the exact branch `main`.

- `agents/fix-login` is allowed.
- `main` is allowed.
- `feature/fix-login` is denied.
- `agents-old/fix-login` is denied.
- tags are denied.

Only branch creation and fast-forward updates are allowed. The proxy validates
commit ancestry itself and rejects history rewrites and branch deletions even
when the upstream permits them. One denied update rejects the whole push.
Fetch, clone, and pull
do not use the write policy.

## Exactly when the check happens

The core cannot check refs when the connection opens because the client has not
sent its push commands yet. A push proceeds in this order:

1. The proxy validates the requested service, provider, and repository path.
2. The core starts upstream `git-receive-pack`.
3. The core forwards the upstream's branch advertisement to the client.
4. The client sends its complete command section. Each command names the old
   object, new object, and ref to update. The packfile comes after this section.
5. The core buffers the command section and checks every ref against the policy.
6. If every ref is allowed, the core sends the command section upstream and
   then streams the packfile.
7. If one ref is denied, the core sends neither the command section nor the
   packfile upstream. It reports every requested ref as rejected and stops the
   upstream process.

For example, if one push contains allowed branch `agents/fix` and denied branch
`wrong`, neither branch is sent upstream.

Provider permissions and branch protection still apply after this check. The
policy can allow a push that the provider later rejects.

Keep the trailing slash in `write_prefix`. Exact branches do not use a trailing
slash. The installer and runtime config loaders reject malformed policy values.
