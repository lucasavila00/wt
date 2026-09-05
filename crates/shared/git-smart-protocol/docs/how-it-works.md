# Request and process flow

The caller gives `serve_git` a client stream, a Git service, and a target. The
target can be a local repository or an SSH server.

The caller has already authenticated the client and approved the repository.
This crate does not do either job.

## Fetch and clone

For `git-upload-pack`, the crate starts the upstream service, sends its ref list
to the client, and copies data in both directions until Git exits. It does not
inspect the fetch request or packfile.

## Push

For `git-receive-pack`, Git first sends a list of ref updates. The crate holds
that list long enough to check every ref.

- If every ref is allowed, the gateway fetches upstream objects into a temporary
  bare repository and stages the incoming pack. Git validates the objects and
  the gateway checks commit ancestry for every updated branch. Only then do the
  original command list and validated pack go to the upstream.
- If one ref is denied, none of them go to the upstream. The client gets a
  rejection for the whole push.

Branch deletions and non-fast-forward updates are rejected independently of
upstream settings. The upstream can still reject an otherwise
allowed push for its own reasons, such as branch protection or a stale ref.

## Targets

- A local target runs Git against `repositories.join(path)`.
- An SSH target uses one private key and one pinned `known_hosts` file. It runs
  in batch mode with strict host-key checking.

`GitTarget` does not validate its fields. The caller must pass a safe path and a
trusted SSH endpoint.

## Extra push messages

Frontends may add a short message after a sideband push. The WT gateway uses
this for hints about published branches. `wt-git-proxy` does not add one.

`successful_push_updates` reads the upstream's result and returns only refs
reported as `ok`. A requested update is not proof that it succeeded.

## Limits

- Git packets are limited to 65,520 bytes.
- A push command list is limited to 1 MiB.
- Captured upstream stderr is limited to 16 KiB.
- Push validation downloads all advertised upstream history on each push and
  temporarily stores it alongside the incoming pack and unpacked objects. This
  adds bandwidth, disk usage and latency proportional to repository size; there
  is no persistent object cache or staging quota.
- Pack framing is read incrementally with fixed-size buffers. Git validates
  checksums, delta bases and object contents; ancestry checks ignore replace refs.
- Push options and signed pushes are rejected; their additional framing is not
  supported by staging.
