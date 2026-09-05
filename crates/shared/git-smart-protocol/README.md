# wt-git-smart-protocol

Shared Git plumbing for WT's two Git frontends.

The crate connects a Git client to a local or SSH upstream. Fetches pass
through. Before a push passes through, the crate reads every requested ref
update and checks it against a branch policy. It stages objects and checks commit
ancestry to reject history rewrites and deletions. If one update is not allowed, the
whole push is rejected before the upstream sees it.

- `wt-agent-tool-gateway` supplies WT world authorization and reporting.
- `wt-git-proxy` supplies standalone OpenSSH and repository configuration.

## How it works

```text
Git client stream
  -> frontend authenticates the client and validates the repository
  -> wt-git-smart-protocol
       -> local Git service
       -> or SSH -> upstream Git service
```

The crate starts `git-upload-pack` for reads and `git-receive-pack` for pushes.
It forwards Git's initial ref advertisement. Fetch data passes through; pushes
are staged and forwarded after validation, with original ref commands preserved.
It can also add an optional message after a successful push.

It does not authenticate clients, validate repository paths, decide which
repositories a client may use, or manage credentials. The caller must do that
before creating a `GitTarget`.

The main entry points are `serve_git`, `WritePolicy`, and `repository_refs`.
The other public helpers support frontend messages and custom streams.

Changes to parsing, process cleanup, or target handling need extra care. This
crate sits on a write and credential boundary.
