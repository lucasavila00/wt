# Show CI trigger events in ag-git

`ag-git list_ci` and `ag-git show_run` do not show which event triggered a CI
run. They currently expose the run ID, state, name, commit, ref, URL, and jobs,
but not whether the run came from `push`, `pull_request`, or another event.

This makes equivalent runs on the same commit and ref appear to be unexplained
duplicates. For example, runs `32463374317` and `32463379335` both appeared for
commit `c7a2fee`, but their output did not identify one as the branch push run
and the other as the pull request run. The distinction had to be inferred from
the repository workflow configuration.

Include the provider's trigger event in `list_ci` and `show_run` output so
agents can distinguish duplicate workflow execution from retries, reruns, and
separate event contexts without leaving ag-git.
