# ADR 0079: Preserve Git protocol framing across optional gateway work

- Status: Accepted
- Date: 2026-08-23

## Context

The agent Git gateway relays `git-receive-pack` responses and may add an
informational sideband message after a successful push. Producing that message
requires optional state and provider inspection after the authoritative Git
response has already been captured.

An inspection failure previously occurred after the gateway had forwarded part
of the response. The outer transport handler then appended a regular `ERR`
packet. A client reading a sideband stream interpreted the first byte of that
payload, `E` (decimal 69), as a sideband channel and reported `bad band #69`.
The branch could still reach the provider even though the client reported that
the push failed.

## Decision

Treat the provider's receive-pack response as the authoritative protocol
result. Optional gateway diagnostics must not change whether that response can
be relayed.

Before writing any receive-pack response bytes, render and packetize the
optional diagnostic into a complete buffer. If rendering or packetization
fails, or there is no diagnostic to add, forward the provider response
byte-for-byte. Insert a successfully encoded message as sideband channel 2
immediately before the provider's terminal flush packet.

Once Git protocol handling begins, propagate errors to the request boundary for
logging and close the stream. Do not append an unframed fallback error: the
gateway cannot safely infer the client's current packet section or sideband
state. Protocol-level rejections that are known before relay remain encoded by
the Git protocol implementation.

Post-push registry updates are also best-effort. Failure to parse the captured
response for those updates is logged after the provider response has been sent
and cannot alter the Git result.

## Consequences

- Optional diagnostics cannot corrupt or replace a valid provider response.
- Clients receive either valid Git framing or a closed connection, never a
  guessed packet appended at an unknown protocol phase.
- Some internal failures are visible only in server logs because no universally
  valid error frame exists after relay begins.
- Response augmentation buffers one receive-pack response plus its diagnostic
  before writing. The response was already captured for post-push inspection,
  so this does not add a new response-sized buffer.
