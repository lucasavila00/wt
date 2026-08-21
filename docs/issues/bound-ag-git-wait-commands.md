# Bound ag-git wait commands and report progress

`wait_mr`, `wait_run`, and `wait_job` poll every ten seconds without a timeout
or intermediate output. If the selected resource does not change or finish,
the command can wait indefinitely while presenting no status to the caller.

This is difficult to distinguish from a stalled command or transport failure,
especially when an outer agent or command runner has its own shorter timeout.

Give wait commands a documented time bound or caller-selected timeout and
return the last observed resource state when that bound expires. If the
transport supports streaming output, periodically report the current state as
well.
