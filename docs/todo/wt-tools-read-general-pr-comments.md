# Let `wt-tools` read general pull-request comments

`show_mr` and `list_threads` expose review threads but omit general pull-request
issue comments. As a result, an agent cannot inspect a linked GitHub
`#issuecomment-*` comment through `wt-tools`, even though it can reply to the PR
with `comment_mr`.

Add an explicit read operation for general pull-request comments, including a
stable comment handle, author, body, URL, and timestamps. It should support
resolving a specific provider comment handle so agents do not need to bypass
the repository-owned tool with provider APIs.
