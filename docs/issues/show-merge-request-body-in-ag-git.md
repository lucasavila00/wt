# Show merge request bodies in ag-git

`ag-git show_mr` and `show_mr_for_branch` show the title, state, head, base, and
URL, but not the merge request body. At the same time, `edit_mr` can replace
that body.

An agent therefore cannot inspect the existing description before deciding
whether or how to edit it. This risks overwriting information that is visible
in the provider but unavailable through ag-git.

Include the complete merge request body in the show commands. Preserve a clear
distinction between an empty body and a body that the provider did not return.
