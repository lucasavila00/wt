# Return unfinished state when a WT tools wait expires

`wt-tools wait_run` returns a nonzero error when its caller-supplied timeout
expires while CI is still `in_progress`. A bounded wait expiring is not a tool
or provider failure. Return the last structured run state successfully; reserve
errors for failures to query or interpret the run.
