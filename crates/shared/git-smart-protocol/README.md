# wt-git-smart-protocol

Shared Git plumbing for WT's two Git frontends.

The crate connects a Git client to a local or SSH upstream. Fetches pass
through. Before a push passes through, the crate reads every requested ref
update and checks it against a branch policy. If one update is not allowed, the
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
It forwards Git's initial ref advertisement, then copies bytes in both
directions. The only protocol content it changes is a rejected push or an
optional message added after a successful push.

It does not authenticate clients, validate repository paths, decide which
repositories a client may use, or manage credentials. The caller must do that
before creating a `GitTarget`.

The main entry points are `serve_git`, `WritePolicy`, and `repository_refs`.
The other public helpers support frontend messages and custom streams.

## More detail

- [How a request moves through the crate](docs/how-it-works.md)
- [How the write policy works](docs/write-policy.md)
- [Where to pay attention when changing it](docs/maintenance.md)

Read the maintenance page before changing parsing, process cleanup, or target
handling. This small crate sits on a write and credential boundary.
