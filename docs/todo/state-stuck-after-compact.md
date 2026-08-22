Not addressed by the session metadata cache.

WT only supersedes a pane's prior session after it receives a non-inactive
report for a replacement session. Verify Codex hook behavior for `/clear` and
compaction, then define how WT marks the old session inactive or transitional
when Codex has changed conversational state but no replacement report arrives.

Preserve ADR 0062's protection against delayed `SessionEnd` events deactivating
the replacement session. Also prevent late non-inactive `UserPromptSubmit` or
`Stop` hooks from the old session from superseding its replacement. Define
liveness reconciliation for a crashed Codex process, dead pane, stopped world,
or dropped hook. Cover `/clear`, compaction, `/reset`, delayed events, and
missing hooks with integration tests.
