#!/bin/sh
set -eu

state=/var/lib/wt-setup
log=$state/install.log
workspace=/workspace
exec >>"$log" 2>&1

test -S "${SSH_AUTH_SOCK:-}" || {
    echo "No forwarded SSH agent is available. Reconnect with agent forwarding enabled." >&2
    exit 1
}

exec 9>"$state/install.lock"
flock 9
test ! -e "$state/complete" || exit 0

source=$(cat "$state/source")
branch=$(cat "$state/git-branch")
git_ref=$(cat "$state/git-ref")
git_name=$(cat "$state/git-user-name")
git_email=$(cat "$state/git-user-email")
export GIT_SSH_COMMAND="ssh -o IdentitiesOnly=no -o StrictHostKeyChecking=yes -o UserKnownHostsFile=$state/git-known-hosts"

if test -d "$workspace/.git"; then
    test "$(git -C "$workspace" remote get-url origin)" = "$source"
else
    find "$workspace" -mindepth 1 -maxdepth 1 -exec rm -rf -- {} +
    git clone "$source" "$workspace"
fi
if test -n "$branch"; then
    git -C "$workspace" fetch origin "$branch"
    git -C "$workspace" checkout -B "$branch" "origin/$branch"
elif test -n "$git_ref"; then
    git -C "$workspace" fetch origin "$git_ref"
    git -C "$workspace" checkout --detach FETCH_HEAD
fi
git -C "$workspace" config user.name "$git_name"
git -C "$workspace" config user.email "$git_email"

additional_features='{"ghcr.io/devcontainers/features/sshd:1":{}}'
devcontainer up --log-level debug --log-format text --workspace-folder "$workspace" \
    --additional-features "$additional_features" \
    --mount type=bind,source=/var/lib/wt-app-ssh/public,target=/run/wt-app-ssh \
    --mount type=bind,source=/var/lib/wt-app-ssh/public/sshd_config,target=/etc/ssh/sshd_config
devcontainer exec --workspace-folder "$workspace" /bin/sh -c \
    'workspace=$(pwd -P) && git config --global --add safe.directory "$workspace"'
/usr/local/bin/wt-app-info > "$state/app.json"
touch "$state/complete"
rm -f "$state/git-known-hosts" /etc/sudoers.d/wt-setup
/usr/bin/tmux set-environment -gu SSH_AUTH_SOCK || true
echo "World setup complete. Open a new pane or reconnect to enter the devcontainer."
