Not addressed by the session metadata cache.

WT only supersedes a pane's prior session after it receives a non-inactive
report for a replacement session. Verify Codex hook behavior for `/clear` and
compaction, then define how WT marks the old session inactive or transitional
when Codex has changed conversational state but no replacement report arrives.

Preserve ADR 0062's protection against delayed `SessionEnd` events deactivating
the replacement session. Cover `/clear`, compaction, `/reset`, and delayed or
missing hooks with integration tests.
