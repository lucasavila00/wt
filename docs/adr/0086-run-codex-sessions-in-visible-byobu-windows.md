# ADR 0086: Run Codex sessions in visible Byobu windows

- Status: Accepted
- Date: 2026-09-03

## Context

Controllers need to delegate work to Codex without holding an API call open for the duration of a
turn. People also need to see the same work in WT's terminal interface and take over through the
normal interactive experience. Codex App Server provides semantic session and turn operations,
while Byobu provides the shared visible terminal already used by worlds.

## Decision

WT starts a guest Codex App Server runtime for each delegated session. It opens a dedicated window
in the world's ordinary Byobu session and runs the native Codex TUI there with `--remote`, connected
to that App Server. The window is visible in `wt shell` and in WT's observed Codex-window UI, and a
person who opens it sees and interacts with the same Codex thread that WT controls semantically.

WT exposes three runtime operations through ADR 0085:

- **Start** accepts a world and initial message, creates the visible window, starts App Server and
  its first turn, and returns a WT runtime session handle as soon as startup is accepted.
- **Inspect** returns the session lifecycle state, active-turn state, and a bounded recent view of
  semantic App Server activity.
- **Send** uses App Server's semantic protocol. It steers the active turn when one is running and
  starts a new turn in the same Codex thread when the session is idle.

These operations address sessions through the WT handle rather than a Byobu index or process ID.
WT validates that the handle belongs to the requested owner and world before acting on it.

## Runtime model

The WT server keeps the associations among world, Byobu window, App Server runtime, Codex thread,
active turn, and recent semantic events in memory. The guest's Byobu session and running processes
hold the corresponding live execution state. Handles are valid for that runtime generation; after
a server or guest restart, a controller treats an unavailable handle as ended and may start a new
visible session.

This model keeps the registry focused on durable world resources and mailbox delivery. The
controller retains its own durable association between its task or sub-session and the WT runtime
handle.

## Completion and mailbox delivery

App Server events drive lifecycle transitions and the native TUI connected to the same runtime
shows the live conversation. When a turn reaches a terminal completed or failed state, WT appends
a terminal message to the world's mailbox as defined by ADR 0084. The mailbox entry provides the
runtime session correlation, status, and final text. The controller imports that durable entry
using the mailbox cursor and may inspect the live session for richer progress while it is running.

When a session ends, WT stops its App Server runtime and closes its dedicated window. Deleting a
world first ends its live sessions through the existing world lifecycle serialization, then
applies the world and mailbox deletion semantics.

## Consequences

- Delegation is asynchronous and leaves API workers free while Codex runs.
- App Server supplies semantic inspection and steering instead of terminal keystroke automation.
- A person can observe and use the native Codex TUI in the same dedicated Byobu window through
  WT's existing terminal surfaces.
- Runtime session state stays small and disposable, while terminal delivery uses the durable world
  mailbox.
- Controllers can rebuild live work by starting a new session and continue durable coordination
  from their imported mailbox history.
