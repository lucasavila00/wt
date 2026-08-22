# Codex session titles fail across the host/guest UID boundary

## Observation

The WT shell shows `Untitled Codex session` for Codex sessions that are
actively running in worlds. Saved sessions may instead show the injected
`AGENTS.md`/initial-instructions text as their title.

This was observed on the live development host on 2026-08-22.

## Confirmed facts

- The host account and `wt-server` service user are `wt`, UID/GID `1000:1000`.
- The golden guest image intentionally owns its `wt` account at the fixed
  UID/GID `1001:1001` (`GUEST_UID`/`GUEST_GID`).
- `wt-server.service` runs as the host `wt` user.
- `/home/wt/.codex/sessions` is shared between the host and guests through
  virtiofs. The host installer gives the host user an ACL and uses guest GID
  `1001` for the shared group.
- A rollout written by a guest was observed as UID/GID `1001:1001`, mode
  `0600`. Its ACL mask was `---`, so the host UID 1000 could not read it.
- The server therefore skipped those rollout files and merged the lifecycle
  reports from the guest gateway. Those report-only sessions have no rollout
  title, so the UI correctly falls back to `Untitled Codex session`.
- The server title parser currently selects the first `response_item` whose
  payload has `role = "user"`. In Codex rollouts, injected initialization
  instructions can appear as that record before the actual user turn, which
  explains saved cards titled with `AGENTS.md` instructions.
- The relevant implementation points are:
  - `assets/server/install-host.sh`: host UID is dynamic, guest GID is
    hard-coded to `1001`.
  - `scripts/bootstrap-server-user`: creates the host `wt` account without a
    fixed UID.
  - `crates/products/wt/server/src/service/codex.rs`: reads rollout files and
    extracts titles.
  - `assets/world/shared/mount-codex.sh`: mounts the shared rollout directory
    in each guest.

## Cause

Virtiofs passthrough preserves numeric ownership. WT fixes the guest identity
at 1001, while bootstrap lets the host allocate the `wt` identity dynamically.
The two accounts have the same name but are different filesystem principals on
this host.

When guest Codex creates a private rollout, its mode and ACL mask can remove
the host account's effective ACL access. Each newly created rollout can
therefore reproduce the failure; it is not specific to one world or rollout.

## Impact

- Live Codex sessions lose their user-visible title when the server cannot
  read their rollout.
- Readable saved sessions can display internal initialization text instead of
  the user's first prompt.
- The same permission failure recurs for private files created by a guest
  identity that differs numerically from the host service identity.
- ACL changes made during server installation do not govern the eventual
  effective access of every file created by guest Codex.

The UID/GID decision is recorded separately in
[ADR 0061](../adr/0061-use-one-wt-identity-across-host-and-guests.md).
