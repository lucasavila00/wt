# ADR 0086: Run Codex sessions in visible Byobu windows

- Status: Accepted
- Date: 2026-09-03

## Context

Controllers need to delegate work to Codex without holding an API call open for the duration of a
turn. People also need to see the same work in WT's terminal interface and take over through the
normal interactive experience. Codex App Server provides semantic session and turn operations,
while Byobu provides the shared visible terminal already used by worlds.

## Decision

Each world has one systemd-supervised Codex App Server. WT attempts a dedicated window for every
delegated Codex thread in the world's ordinary Byobu session and runs the native Codex TUI there
with `--remote`, connected to that daemon. The window is visible in `wt shell` and in WT's observed
Codex-window UI, and a person who opens it sees and interacts with the same Codex thread that WT
controls semantically.

WT exposes four runtime operations through ADR 0085:

- **Start** accepts a world and initial message, starts the thread and first turn, creates the
  visible window, and returns the Codex thread ID, turn ID, and pane metadata when startup is
  accepted.
- **Inspect** reads the App Server thread state and returns its active turn together with the
  optional current pane metadata and captured screen.
- **Send** starts a new turn in the same Codex thread when it is idle or its prior turn failed,
  and rejects a busy thread without steering it.
  A retained `systemError` status does not prohibit an explicit new message; Codex validates it.
- **Resume** loads a retained thread and attempts to restore its window without submitting work.

Pane metadata is optional on start, inspect, and resume. Pane creation/capture failures do not
fail semantic thread operations, and send does not require a pane.

These operations address sessions through the opaque Codex thread ID. WT validates the requested
owner and world before asking that world's guest runtime to resolve the thread.

## Runtime model

App Server owns live thread and turn state. Each Byobu pane carries its thread ID as a tmux option,
which lets WT find and capture the corresponding native TUI. A `wts` restart leaves this guest-local
state running. A guest restart ends the live runtime generation, but per-world Codex thread
history persists (ADR 0082). The explicit `resume_codex` mutation loads a retained thread ID and
reopens its visible pane when absent. It returns the same state and screen shape as inspection,
without submitting a message, starting a turn, or interrupting an active turn. A repeated call
reuses a live pane. Send remains a separate operation after recovery. Inspection remains
read-only, returning absent pane/screen metadata until presentation is restored. Resume does not
restart an interrupted turn or replay its prompt.

The service uses documented `codex app-server --listen unix:///...` and
`codex --remote unix:///... resume THREAD_ID` commands, not hidden daemon/proxy commands. Both the
App Server and completion worker start after history/authentication mounts and restart on failure.
This is a documented integration surface, not a stability guarantee: OpenAI labels the WebSocket
transport experimental. `.codex-version` is the single runtime version source for guest image
installation and CI. Both verify the installed executable reports that version. Upgrade by
changing that file and passing the real-runtime compatibility checks before rebuilding images;
pinning limits compatibility drift, not the experimental support status. See the
[official App Server documentation](https://learn.chatgpt.com/docs/app-server) and
[CLI developer commands](https://learn.chatgpt.com/docs/developer-commands?surface=cli).

WT explicitly requests legacy thread history with the experimental capability enabled. The
pinned runtime defaults to paginated history whose full read/resume operations are not implemented.
Fresh legacy threads are not readable/resumable until materialized: register their known-empty
baseline first, submit the initial turn, then attempt to attach the TUI. Never infer history mode
from the presence of a thread ID or bypass unsupported history by scraping rollout files.

Resume requires a running world and the normal expected-server identity check and per-world
operation lock. Request-ID replay returns the original result; use a new request ID for a later
restart. Missing thread history is an error, never an implicit replacement conversation.
Start/send carry mutation hashes like other writes. An uncertain guest failure is cached as
nonretryable: Codex may have accepted work before the response was lost. Inspect and reconcile
before submitting new work; changing the request ID is not an automatic retry strategy.
Codex reservations survive server startup cleanup if saving the response failed or the server
crashed. Replaying such an unresolved request cannot resubmit work; reconcile through inspection
and mail. This protection has the API's existing 30-day request-retention window.

The controller retains its durable association between its task or sub-session and the Codex
thread ID. The WT registry stores world resources and mailbox entries.

## Explicit turn control

`send_codex_message` starts a follow-up only when there is no active turn. A busy thread
returns a retryable conflict before submission; it never implicitly steers. Controllers such
as APR own their durable FIFO queue, rather than duplicating scheduling inside each guest.
`steer_codex` requires the expected active `turn_id`; a stale target fails without starting
replacement work. `interrupt_codex` also targets a turn and returns `interrupt_requested`.
This is acknowledgment of the request, not terminal completion: wait for the normal mailbox
result or inspect the thread. Interrupt does not cancel a controller's queued follow-ups.
Both controls retain normal world ownership checks and mutation replay protection.

## Completion and mailbox delivery

Before submitting work, WT atomically registers the thread in its guest-local durable tracking
directory. Already-terminal history is the initial baseline; an active turn remains eligible for
delivery. Repeated send/resume never resets tracking. Subsequent human-TUI turns are tracked too.

A supervised worker reconciles `thread/read` snapshots, subscribes active threads with
`thread/resume`, and releases idle subscriptions. It does not depend on receiving every event.
Each terminal payload is persisted before relay delivery and retried after outages or worker
restarts; acknowledgment is durably recorded. Settled turn IDs avoid losing results when history
changes. The gateway transactionally deduplicates by world/thread/turn, including lost-ACK retries.
Pane IDs are optional metadata. This guarantees one mailbox insertion per tuple, not exactly-once
execution of Codex work. Controllers import durable results through the normal mailbox cursor.

If retained history contains an in-progress turn with no loaded runtime, WT emits an explicit
recovery failure with the original turn ID: the final outcome is unavailable. This is not a Codex
interruption event, history rewrite, or prompt resubmission. Transport errors alone do not establish
interruption. Inspection works independently of terminal availability.

Deleting a world ends its guest runtime and applies the world and mailbox deletion semantics.

## Consequences

- Delegation is asynchronous and leaves API workers free while Codex runs.
- App Server supplies semantic inspection and steering.
- A person can observe and use the native Codex TUI in the same dedicated Byobu window through
  WT's existing terminal surfaces.
- Live execution is guest-local; retained history and delivery tracking survive guest restarts.
- Controllers can resume the original thread and continue coordination from imported mail.
