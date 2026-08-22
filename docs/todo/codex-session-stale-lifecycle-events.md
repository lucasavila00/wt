When `/clear` creates a replacement Codex session in one pane, a late
non-inactive `UserPromptSubmit` or `Stop` from the old session can supersede
the replacement. Preserve ADR 0062's delayed-`SessionEnd` protection and make
all late old-session events harmless.

Receipt time cannot establish causality: delayed or duplicate events from the
same session can also overwrite a newer state. Add an authoritative per-pane
generation or monotonic hook sequence from the world, then reject stale events
against that fence rather than inventing state precedence.

Define liveness reconciliation for a crashed Codex process, dead pane, stopped
world, or dropped hook so a session cannot remain active or needs-attention
forever. Cover `/clear`, `/reset`, delayed events, and missing hooks with
integration tests.
