#!/bin/sh
set -eu

mode=${1:-}
project=$(git config --local wt.project)
case "$project" in
ssh://*) project=${project#ssh://}; project=${project#*@}; project=${project#*/} ;;
*) project=${project#*:} ;;
esac
project=${project%.git}
base=$(git config --local wt.base)
prefix=$(git config --local wt.prefix)
branch=$(git symbolic-ref --quiet --short HEAD 2>/dev/null || true)

cat >&2 <<EOF
WT: This is a WT-managed development environment for a coding agent.
WT: For safety, the developer's SSH keys and GitHub or GitLab credentials are
WT: not available here. Do not look for credentials or use gh or glab.
WT: WT gives you scoped access to project $project.
WT: Use normal Git for commits, fetches, pulls, and pushes. Every branch you
WT: push must start with $prefix. Pull or merge requests target $base.
WT: ag-git is the installed CLI for pull or merge requests, reviews, and CI.
WT: Run ag-git for the current branch's status and suggested next actions.
WT: Run ag-git --help to discover every available command.
WT:
EOF

case "$mode" in
checkout)
    case "$branch" in
    "$prefix"*) ;;
    '') ;;
    *)
        cat >&2 <<EOF
WT: This world can only push branches that start with $prefix.
WT: Rename the current branch before pushing:
WT:   git branch -m ${prefix}fix-name
EOF
        ;;
    esac
    ;;
commit)
    cat >&2 <<EOF
WT: Commit created on $branch.
WT: Publish it with:
WT:   git push
WT: After pushing, run ag-git to open or manage its pull or merge request.
EOF
    ;;
*)
    echo "usage: wt-agent-git-hint checkout | commit" >&2
    exit 2
    ;;
esac
