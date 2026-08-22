# ADR 0064: Retry world inspection and report failures

- Status: Accepted; Date: 2026-08-22

## Decision

World reconciliation treats inspection transport failures as transient. For a
running or stopped world, the server makes one inspection attempt and up to six
retries, waiting ten seconds between attempts. It does not change the stored
world status while those retries are in progress.

An inspection result remains authoritative: a reported running, stopped, or
missing world is applied immediately. If all inspection attempts fail, the
server records the world as errored with the final inspection error. An
exhausted inspection is not described as proof that the world is powered down.

Later inventory requests inspect errored worlds once. A successful inspection
can therefore restore the stored running or stopped state without requiring a
manual lifecycle operation. These recovery probes are not given the full retry
delay, so one previously failed world cannot repeatedly stall every inventory
request.

If a Worlds refresh fails, the UI retains the last complete snapshot and does
not advance its freshness time. Worlds and Codex use the same title component
and failure shape: `<panel> · Last updated <timestamp> · Sync failed: <reason>`.
Before any snapshot has succeeded, the title uses `Updating…` in place of the
timestamp. Reasons identify the affected contexts and use concise, sanitized
transport summaries joined with `; ` rather than multiline diagnostic details.
The warning is confined to that title line; cards and the rest of the layout
retain their existing content and interaction.

The UI does not show an intermediate `Retrying…` state because the list protocol
does not expose inspection-attempt events. A later complete refresh clears the
warning and advances the title's timestamp. Persisted world errors in a
successful snapshot continue to use the existing card details; they do not turn
that complete refresh into a title-level refresh failure.

## Consequences

- A transiently unresponsive guest agent can delay a list request by about one
  minute, but does not immediately turn a healthy, CPU-bound world into an
  errored world.
- Persistent inspection failures remain visible in world details. A failed
  refresh is not mistaken for a fresh inventory because its title retains the
  timestamp of the data still on screen.
- Recovery is automatic on a later refresh, and a complete refresh clears the
  title warning.
