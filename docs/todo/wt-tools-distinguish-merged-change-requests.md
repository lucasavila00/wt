# Let `wt-tools` distinguish merged change requests from closed ones

`show_mr` currently reports a merged change request as `closed`, without a
separate merge indicator or merge commit. An agent waiting for CI cannot tell
whether a change request was merged while its jobs were still running or was
closed without merging.

Expose a distinct merged state, or equivalent merge metadata including the
merge commit, through `show_mr` and the waiting commands. Keep ordinary closed
change requests distinguishable so agents can report the correct outcome.
