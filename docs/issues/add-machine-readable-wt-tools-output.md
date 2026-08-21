# Add machine-readable wt-tools output

`wt-tools` accepts typed JSON command objects but returns only
human-formatted text.
Agents must parse positional lines such as `run 123 [queued] CI` to recover
resource IDs and states.

That parsing is fragile when output gains fields, provider values contain
unexpected text, or GitHub and GitLab expose different data. It also prevents
callers from reliably distinguishing absent values from display placeholders
such as `unknown` and `unavailable`.

Add a machine-readable output mode that returns a stable, versioned JSON shape
for merge requests, review threads, CI runs, jobs, logs, confirmations, and
errors. Keep the current text output for interactive use.
