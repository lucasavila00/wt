# ADR 0086: Run Codex sessions in visible Byobu windows

- Status: Accepted
- Date: 2026-09-03

## Context

Controllers need to delegate work to Codex without holding an API call open for the duration of a
turn. People also need to see the same work in WT's terminal interface and take over through the
normal interactive experience. Codex App Server provides semantic session and turn operations,
while Byobu provides the shared visible terminal already used by worlds.

## Decision

Each world has one guest-local Codex App Server daemon. WT opens a dedicated window for every
delegated Codex thread in the world's ordinary Byobu session and runs the native Codex TUI there
with `--remote`, connected to that daemon. The window is visible in `wt shell` and in WT's observed
Codex-window UI, and a person who opens it sees and interacts with the same Codex thread that WT
controls semantically.

WT exposes three runtime operations through ADR 0085:

- **Start** accepts a world and initial message, starts the thread and first turn, creates the
  visible window, and returns the Codex thread ID, turn ID, and pane metadata when startup is
  accepted.
- **Inspect** reads the App Server thread state and returns its active turn together with the
  current pane metadata and captured screen.
- **Send** uses App Server's semantic protocol. It steers the active turn when one is running and
  starts a new turn in the same Codex thread when the session is idle.

These operations address sessions through the opaque Codex thread ID. WT validates the requested
owner and world before asking that world's guest runtime to resolve the thread.

## Runtime model

App Server owns live thread and turn state. Each Byobu pane carries its thread ID as a tmux option,
which lets WT find and capture the corresponding native TUI. A `wts` restart leaves this guest-local
state running. A guest restart ends the runtime generation; a controller treats a thread without
its pane as ended and may start a new visible session.

The controller retains its durable association between its task or sub-session and the Codex
thread ID. The WT registry stores world resources and mailbox entries.

## Completion and mailbox delivery

The native TUI shows the live conversation. A short-lived guest watcher consumes the App Server
`turn/completed` event for each WT-started turn and appends a terminal message to the world's
mailbox as defined by ADR 0084. The mailbox entry provides the thread ID, turn ID, pane ID, status,
and final text. The controller imports that durable entry using the mailbox cursor and may inspect
the live session while it is running.

Deleting a world ends its guest runtime and applies the world and mailbox deletion semantics.

## Consequences

- Delegation is asynchronous and leaves API workers free while Codex runs.
- App Server supplies semantic inspection and steering.
- A person can observe and use the native Codex TUI in the same dedicated Byobu window through
  WT's existing terminal surfaces.
- Runtime session state is guest-local and disposable, while terminal delivery uses the durable
  world mailbox.
- Controllers can rebuild live work by starting a new session and continue durable coordination
  from their imported mailbox history.
