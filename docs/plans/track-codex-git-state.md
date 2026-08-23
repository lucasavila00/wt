# Track Codex Git state independently of lifecycle hooks

## Status

Proposed after a live investigation on 2026-08-23. This plan does not change
Codex lifecycle semantics. It separates repository discovery from those
semantics so a session card can follow branch changes while a turn is running.

## Problem

WT currently discovers repository metadata inside `wt-codex-integration
report-hook`. Every Codex lifecycle event runs these commands against the hook's
working directory:

```text
git rev-parse --show-toplevel
git config --get remote.origin.url
git branch --show-current
```

The resulting repository root, origin URL, and branch are stored in the same
`codex_session_reports` row as the lifecycle state. They are refreshed only on
`SessionStart`, `PreCompact`, `PostCompact`, `UserPromptSubmit`, `Stop`, or
`SessionEnd`.

This makes Git metadata a snapshot of the last lifecycle event, not current
Git state. A branch can change during a long-running turn without causing any
of those events. The card continues to show the old branch and gives no
indication that the value is stale.

The host cannot solve this by reading the recorded working directory directly.
The path names a directory inside the KVM guest's disk; only the guest can read
that checkout. Polling it from each `wt shell` client would also duplicate work,
add SSH fan-out to every refresh, and make results depend on which client is
open.

## Investigation

The reported card was:

```text
󰔟 WORKING · 18m ago · ars.calm-wombat · main
```

The live host and guest showed the following sequence:

| UTC | Evidence |
| --- | --- |
| 16:33:19 | The host registry accepted generation 2, sequence 14 for session `01a02f77-83d5-71a2-9fdd-61ae49ed9f67` in `calm-wombat`. It stored state `working`, cwd `/home/wt/wt`, and branch `main`. |
| 16:36:03 | The guest reflog recorded a checkout from `main` to `wt/read-general-pr-comments`. No Codex lifecycle event occurred at the checkout. |
| 16:51:29 | The guest committed on `wt/read-general-pr-comments`, while the registry still held the earlier snapshot. |
| 16:55:45 | A new prompt invoked the hook and advanced the guest pane order. Codex's log recorded `hook/started` followed by `hook/completed`. |
| 16:57:13 | Later lifecycle reporting reached generation 2, sequence 17. The registry then held branch `wt/read-general-pr-comments` and state `inactive`. |

The guest relay remained active throughout. Its only journal error was an
unrelated broken pipe at 16:02. The hook configuration contained all expected
lifecycle commands, the Codex and relay processes were alive, the pane marker
was valid, and the checkout was healthy. The tracker had not died: it had no
event on which to observe the branch switch.

Two diagnostic details made the symptom more confusing:

- Hooks intentionally fail open. `wt-codex-integration` suppresses
  `report-hook` errors and exits successfully, so an actual failure has no
  durable hook-local diagnostic.
- The KVM host also had an unrelated stale client at
  `/home/wt/.cargo/bin/wt` at commit `9b326b3f`. It expected control protocol 9
  while `/usr/local/bin/wt` and the running `wt-server`, both at commit
  `1914c41c`, used protocol 10. The compatible binary listed `calm-wombat`
  normally. This PATH mismatch can prevent client refreshes but did not cause
  the stored `main` value. The `ars` prefix was also not stored by the server:
  context names are client-side, as designed.

## Design

### Keep lifecycle hooks authoritative for session identity

Codex hooks continue to report only information Codex owns:

- session ID and working directory;
- Byobu session and pane;
- pane generation and lifecycle sequence;
- lifecycle state, compaction phase, and session-start source.

Remove Git discovery from `report-hook`. This keeps hooks short and prevents a
slow or broken checkout from delaying activity reporting. A successfully
forwarded non-`SessionEnd` event registers or refreshes that session with the
guest Git tracker. `SessionEnd` unregisters it.

The working directory is stable for a running Codex registration, but every
lifecycle event may replace it. A replacement is applied only after the host
accepts the lifecycle event, so a rejected or delayed event cannot redirect
tracking.

### Track registered directories inside the guest

Extend the long-running `wt-agent-tool-gateway-relay` process to own one Git
tracker per guest. It already:

- receives successful Codex lifecycle registrations;
- validates the exact `wt-host` pane;
- holds the guest grant used to reach the host gateway;
- runs as the unprivileged `wt` user with access to guest repositories.

Maintain a shared map keyed by session ID containing cwd, tmux session, pane,
and pane generation. Persist the map atomically under
`~/.local/state/wt/codex-git-tracker.json` with mode `0600`, and reload it after
a relay restart. On reload and before every poll, retain an entry only when its
pane still exists and its `@wt_codex_session_id` marker matches the session.
This prevents a crashed relay from reviving an old pane registration.

Poll every two seconds and immediately after a new registration. Polling is
preferred over inotify because Git worktrees may use a `.git` indirection file,
checkout and detach operations replace `HEAD`, repositories can appear or
disappear at the cwd, and watching all relevant ref paths is more complex than
the bounded command set. Run `/usr/bin/git` with a short timeout and bounded
stdout/stderr. Do not overlap polls for the same registration.

For each cwd, discover the repository root, origin URL, and current symbolic
branch. A valid non-repository result clears all three fields. A detached HEAD
keeps the repository fields and clears only the branch. Send a report
immediately when the normalized result differs from the last successful result;
also send the first result after registration or relay restart. Send an
unchanged health heartbeat every 15 seconds so the host can distinguish a quiet
checkout from a dead or disconnected tracker.

### Separate Git updates from lifecycle ordering

Add an authenticated gateway operation carrying:

```text
session_id, cwd, tmux_session, pane_id, pane_generation,
repository_root?, repository_url?, git_branch?, error?
```

The world ID continues to come exclusively from the gateway grant. The Git
operation has no lifecycle sequence and cannot create a report. The host accepts
it only when an existing non-inactive report matches the exact world, session,
cwd, target, and pane generation. A delayed tracker result for a replaced
session is therefore ignored.

Update only repository metadata and new Git-specific diagnostic fields. Never
change lifecycle state, compaction, pane ordering, session-start source, or the
lifecycle `received_at_unix_ms`. In particular, a Git poll must not make an old
`WORKING` report appear recently active.

Add these nullable columns to `codex_session_reports`:

```text
git_context_checked_at_unix_ms
git_context_error
```

On successful discovery, replace repository root, URL, and branch, set the Git
check time, and clear the error. On command, timeout, persistence, or transport
failure, retain the last successful repository values, update the Git check
time when the host can be reached, and store a bounded sanitized error. The
control protocol keeps its existing repository fields and adds the optional Git
check timestamp and error to each observation.

Once an observation has received its first tracker result, the shell treats a
Git check time older than 30 seconds as stale and renders a warning even when
the last report contained no error. A missing timestamp means legacy snapshot
behavior because an older world has no tracker; it is not itself an error. This
heartbeat rule makes relay or transport failure visible without requiring the
failed guest to deliver an error report.

This gateway change is additive. Keep the existing agent-gateway protocol
version so older worlds can continue sending lifecycle events. They retain the
current snapshot behavior until recreated from an image containing the new
relay. New servers accept both old lifecycle events and the new metadata
operation.

### Make failures visible

An observation with `git_context_error` renders its last successful repository
metadata as stale and adds a concise `Git state unavailable: ...` line. It must
not silently present the last branch as current. A successful later poll clears
the warning without affecting lifecycle age. An expired Git heartbeat renders
`Git state stale` with its last check age and likewise marks retained metadata
as stale.

Persist the last `report-hook` failure separately at
`~/.local/state/wt/codex-session-report-error.json`, using an atomic `0600`
replacement containing timestamp, event name, session ID, and bounded error.
Clear it after the next successful lifecycle report. Continue to fail open so a
WT reporting problem never blocks Codex.

The relay logs tracker persistence, Git execution, and transport errors to its
systemd journal with session and pane identifiers but without repository URLs
or command output that may contain credentials. Repeated identical errors are
rate-limited.

The stale host client is a separate installation/PATH issue. Document the
compatible-binary check in operational diagnostics, but do not couple client
version repair to Git tracking.

## Implementation sequence

1. Add the metadata operation and registry update method, including exact-row
   validation and the two-column migration. Extend the control response with
   Git check health while preserving lifecycle receipt time.
2. Refactor Git discovery out of `report-hook` into a reusable bounded helper.
   Add registration persistence, pane-marker validation, and the two-second
   polling loop to the guest relay. Register only after a successful lifecycle
   response and unregister on a successful matching `SessionEnd`.
3. Render Git errors and stale metadata in Codex and live-session cards. Keep
   card title age tied to lifecycle activity.
4. Add the last-hook-error state file and rate-limited relay journaling. Update
   operator and architecture documentation to explain independent lifecycle and
   Git freshness.
5. Publish the new guest binaries in the retained image. Existing worlds remain
   usable with snapshot semantics; recreate a world to opt into continuous Git
   tracking.

## Verification

Unit and snapshot coverage must prove:

- a lifecycle event stores state without running Git and registers tracking
  only after the host accepts it;
- an immediate poll discovers a normal branch, a detached HEAD, a Git worktree,
  a repository without `origin`, and a non-repository cwd;
- switching branches during one uninterrupted `working` turn updates the card
  within one five-second shell refresh without changing lifecycle age or pane
  sequence;
- a delayed Git result cannot update a different session, cwd, pane generation,
  inactive report, or deleted world;
- relay restart reloads valid registrations and rejects missing or mismatched
  pane markers;
- Git timeouts and malformed output retain the last successful metadata, expose
  a sanitized error, write a journal diagnostic, and recover on the next
  successful poll;
- an interrupted relay or host transport stops Git heartbeats and produces a
  stale warning within 30 seconds without changing lifecycle age;
- lifecycle delivery failure writes the bounded guest error file, remains
  fail-open, and a later success clears it;
- older lifecycle-only relay requests remain accepted and return the existing
  snapshot behavior;
- card snapshots distinguish lifecycle report age from Git check health.

Run formatting, tests, and Clippy for the affected registry, gateway,
integration, control-protocol, server, and client crates. Run shell syntax and
ShellCheck for changed image assets. Add one targeted real-system KVM test that
starts Codex, switches the checkout branch while its turn remains active, and
observes the new branch through `list_codex_sessions` without another Codex
hook. Finish with `make ci`; keep the repository's four-job Cargo and Rust test
thread limits unchanged.

## Acceptance criteria

- Session activity and Git freshness are independent data flows.
- A branch change in a registered guest checkout appears within five seconds
  even when Codex emits no lifecycle event.
- Git updates never refresh or reorder lifecycle activity.
- The host never reads guest repository paths directly, and clients do not poll
  guests individually.
- Tracker and hook failures leave durable, sanitized evidence and cannot leave
  an old branch looking current without a warning.
- Old worlds remain operational; continuous tracking begins after recreation
  from the updated retained image.
