After Codex compaction finishes, WT can remain in the `COMPACTING` state
instead of returning to `WORKING`. Identify the post-compaction hook/state
transition and make the displayed session state converge to the active Codex
state. Cover compaction completion with an integration test.
