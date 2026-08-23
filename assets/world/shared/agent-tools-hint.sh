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
WT: WT gives you read access to every repository available to the Git gateway.
WT: This checkout is project $project and its configured base is $base.
WT: Use normal Git for commits, fetches, pulls, and pushes. Every WT world can
WT: write branches under $prefix in any available repository.
WT: wtg tools uses explicit provider resource types and IDs; it does not infer
WT: resources from the current checkout.
WT: Run wtg tools --help to discover every available command.
WT:
EOF

case "$mode" in
checkout)
    case "$branch" in
    "$prefix"*) ;;
    '') ;;
    *)
        cat >&2 <<EOF
WT: Branches pushed from a WT world must start with $prefix.
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
WT: After pushing, use the explicit wtg tools commands printed by the Git gateway.
EOF
    ;;
*)
    echo "usage: wt-agent-tool-gateway-hint checkout | commit" >&2
    exit 2
    ;;
esac
