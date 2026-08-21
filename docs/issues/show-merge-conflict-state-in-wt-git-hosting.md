# Show merge conflict state in wt-git-hosting

The `show_mr` and `show_mr_for_branch` JSON actions show the merge request's
title, state, head, base, and URL, but not whether the head can merge cleanly
into the base branch.

An agent therefore has to fetch and merge the base branch locally to discover
conflicts. That is useful before updating or handing off a pull request, but it
also changes the branch and cannot serve callers that only need a read-only
status check.

Include the provider's mergeability or conflict state in both show commands.
Represent pending provider computation distinctly from cleanly mergeable and
conflicting states.
