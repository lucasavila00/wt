# ADR 0068: Queue shell actions

- Status: Proposed; Date: 2026-08-23
- Amends: [ADR 0052](0052-add-a-world-and-session-tui.md) and
  [ADR 0053](0053-use-a-shared-world-creation-form.md)

## Decision

### Default

`ShellActionQueue` is the default execution path for every user-initiated
`wt shell` command or action. An action may bypass it only when its complete
effect is local to the in-memory UI or terminal model, synchronous, and
bounded. If it starts work, performs I/O, waits for readiness, mutates external
state, or completes through a later event, it enters the queue. Uncertainty
means queue it.

### Entries

`wt shell` owns one in-memory FIFO `ShellActionQueue`. Each entry is a typed,
immutable intent with a never-reused local ID, stable user-facing goal, and
enough stable identity to avoid retargeting deferred work. Forms and pickers
build intents; confirmation enqueues them. They do not own work.

Create/delete world, open/focus a Codex session, and reconnect world playback
are the initial actions. Future lifecycle, synchronization, update, helper-backed,
or asynchronously completed commands use the same queue.

### Execution

The UI event loop owns queue state and is the only component that selects and
activates an entry. It starts one worker at a time, applies only ID-matched
events, and activates the next entry only after the previous terminal result
and required shell-state updates have been applied. Blocking work runs in
workers; they never inspect or advance the waiting queue.

The event loop already owns shell state, sessions, input, and rendering. A
worker or coordinator cannot know that those updates succeeded without a second
acknowledgement protocol and duplicate queue state.

No user command handler starts an ad-hoc worker. Read-only form preparation and
picker queries use their own background workers and never block the UI.

### Presentation and recovery

The queue is the only progress lifecycle. Its shared view shows the active goal
and phase followed by waiting goals in FIFO order. A queued action can be
removed. Each action defines its failure and cancellation behavior before it is
added. Failed or uncertain work is never replayed automatically; it blocks the
head until recovery or explicit discard completes.

An active action is cancellable only when it has end-to-end cancellation and
acknowledgement. Provisioning does not yet have that contract, so closing its
progress view remains hide-only.

### Explicit exceptions

These are the complete current exceptions to the default rule:

- Local UI actions: navigation, selection, scrolling, palettes, form editing,
  switching to an already-open playback buffer, queue controls, and quit.
- Terminal data-plane work: input/output forwarding and resize.
- Background maintenance: refresh and automatic view maintenance. It yields to
  a queued or active action over the same resource, and its stale results are
  ignored.
- Standalone commands outside `wt shell`.

The queue is local to `wt shell` and is lost when it exits. `wt-server`
continues to execute normal individual operations; it does not store or schedule
queue state.
