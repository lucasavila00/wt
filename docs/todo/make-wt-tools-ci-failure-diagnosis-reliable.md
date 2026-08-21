# Make wt-tools CI failure diagnosis reliable

`wt-tools log_job` repeatedly failed for completed, failed GitHub Actions jobs
with:

```text
read the WT Git gateway response; the relay or gateway may have stopped: read
request header: failed to fill whole buffer
```

This affected jobs `96871711041`, `96875501519`, and `96875501889`. Repeating
the call did not recover, while `show_job`, `list_jobs`, and `list_ci` continued
to work. The missing log forced local reproduction and made a test failure look
like runner instability.

Return a bounded log tail or job annotations when the complete log cannot cross
the relay. The error should distinguish a provider failure, an oversized or
truncated response, and a stopped gateway.

Also make expected polling timeouts machine-distinguishable from tool failures.
`wait_run` currently returns the same top-level error shape when a healthy run
is merely still in progress after `timeout_seconds`.

Finally, ensure `show_mr` reports current jobs consistently with `list_ci`;
during this investigation `show_mr` returned `jobs: []` for a commit whose run
and jobs were visible through `list_ci`.
