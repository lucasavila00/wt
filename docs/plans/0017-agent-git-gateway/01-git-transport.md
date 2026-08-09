# Stage 1: Git transport and world integration

This stage makes normal Git work from every world. It does not call a
GitHub or GitLab API.

Implement it in this order:

1. Replace the current SSH-agent Git path in the client request, server state,
   and world lifecycle. Require a branch base and record the project, base,
   world prefix, and gateway grant. Reject `wt fork` for every world.
2. Delete WT's SSH-agent forwarding code. Remove `ForwardAgent` from generated
   SSH config, remove every `SSH_AUTH_SOCK` handoff, and remove the guest and
   devcontainer socket plumbing. The gateway replaces the agent for both the
   initial clone and later Git operations. Do not block native forwarding on an
   explicit `ssh -A NAME-host` or `ssh -A NAME-vs` connection; those paths are
   outside WT.
3. Add the gateway, guest relay, `git-remote-ag`, and `ag-git` to the workspace
   and installer as unconditional WT components. Connect `wt-server` to the
   gateway by Unix socket and each guest relay to it by KVM vsock.
4. Make `ag-git` a POSIX shell frontend that forwards arguments, stdin, stdout,
   stderr, and exit status. Keep command parsing, help, and behavior in the
   gateway. Keep `git-remote-ag` and the guest relay limited to transport.
5. Add the provider configuration from the main plan. Require at least one
   provider, validate and install its SSH key, and reject repository hosts whose
   provider is not configured.
6. Issue one revocable gateway grant when a world is created. Store it on the
   world's private disk outside the checkout. Revoke it before deleting the
   world.
7. Configure `origin` with an `ag::` URL. Stream the real Git protocol through
   the remote helper, relay, and gateway so ordinary `git fetch`, `git pull`,
   and `git push` work.
8. Enforce the world's branch prefix at the gateway. Allow updates,
   force-pushes, and deletion inside that prefix. Reject tags and every other
   branch with the message from the ADR.
9. Install the checkout and commit hints without replacing project hooks.
   Snapshot the gateway's complete `ag-git --help` response and all user-visible
   messages. Provider-backed commands fail clearly until stage 2 or 3
   implements the selected provider.
10. Update `DEVELOPMENT.md`, `docs/how/cli.md`, and
   `examples/server-config/wt-server.development.toml` with the GitHub-only
   development setup from the main plan.

## E2E test

The test harness runs the real gateway core against a temporary local bare Git
repository. That local upstream is not a user-facing provider configuration.
The test also runs the real WT server, VM, guest relay, devcontainer, and Git
client.

The test uses no network service, SSH agent, GitHub or GitLab account, SSH key,
or provider credential. It must prove that a world can clone, fetch, push,
force-push, and delete its own branch with normal Git. It must also prove:

- creation succeeds with `SSH_AUTH_SOCK` unset;
- generated SSH aliases do not enable `ForwardAgent`;
- the guest and devcontainer have no forwarded agent socket;
- another prefix and tags are rejected;
- a rejected push leaves the upstream unchanged;
- a gateway restart keeps namespace ownership;
- changing a gateway response changes `ag-git` output in the existing world;
- a deleted world's grant no longer works.

Read the bare repository directly to verify refs and objects. Run this test in
the existing KVM E2E suite.

## Completion

Stage 1 is complete when the E2E test passes and the installed help and
discoverability text describe the final workflow. Pull and merge request
operations do not work yet.
