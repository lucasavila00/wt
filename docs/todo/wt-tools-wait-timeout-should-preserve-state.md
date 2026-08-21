# Preserve state when a wt-tools wait times out

`wt-tools wait_run` returns an error when its timeout expires even if the CI run
is healthy and still `in_progress`. This makes normal bounded polling look like
a tool or provider failure.

Return a successful timeout result containing the last observed state. Reserve
errors for failures to query or interpret the provider response. Apply the same
behavior to `wait_mr` and `wait_job`, and cover all three commands with tests.

Observed with GitHub run `32514377167`: a 55-second wait ended with `last state:
in_progress` inside an error response.

Relevant code: `crates/products/agent-tools/tools`.
