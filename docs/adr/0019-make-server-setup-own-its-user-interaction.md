# ADR 0019: Make server setup own its user interaction

- Status: Accepted
- Date: 2026-08-10

## Context

Server setup streams output and prompts from the tools it calls. A
passphrase-protected Git key therefore produces `Enter old passphrase:` with no
explanation of which key WT is using, why it needs the passphrase, or whether it
will modify the key.

## Decision

Server setup presents concise, named phases and reports failures in the phase
where they happened. Routine package and build output stays quiet unless there
is an error.

WT owns every credential prompt. For an encrypted provider SSH key, setup names
the provider and key, explains why the gateway needs an unlocked copy, and says
what will happen to both copies. WT reads the passphrase without echoing it and
unlocks the key directly. It never changes the source key or passes the
passphrase to another process.

Setup remains safe to rerun after interruption or failure.
