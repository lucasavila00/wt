#!/bin/sh
set -eu

state=/var/lib/wt-setup
log=$state/install.log
workspace=/workspace
inner=false
test "${1:-}" != --inner || inner=true

if ! "$inner"; then
    # tee makes child output non-TTY; preserve terminal rendering when a pane is attached.
    rich=false
    test -t 1 && test "${TERM:-dumb}" != dumb && rich=true
    pipe=$state/install-log.$$
    mkfifo "$pipe"
    exec 3>&1
    tee -a "$log" < "$pipe" >&3 &
    tee_pid=$!
    exec > "$pipe" 2>&1
    finish_log() {
        status=$1
        trap - 0
        exec 1>&3 2>&3
        wait "$tee_pid"
        rm -f "$pipe"
        return "$status"
    }
    trap 'status=$?; finish_log "$status"; exit "$status"' 0

    if "$rich"; then
        script -qefc '/usr/local/bin/wt-setup-world --inner' /dev/null
        finish_log 0
        exec /usr/local/bin/wt-app-pane
    fi
fi

exec 9>"$state/install.lock"
flock 9
if test -e "$state/complete"; then
    exit 0
fi

if test -e "$state/source"; then
    test -S "${SSH_AUTH_SOCK:-}" || {
        echo "No forwarded SSH agent is available. Reconnect with agent forwarding enabled." >&2
        exit 1
    }
    source=$(cat "$state/source")
    branch=$(cat "$state/git-branch")
    git_ref=$(cat "$state/git-ref")
    git_name=$(cat "$state/git-user-name")
    git_email=$(cat "$state/git-user-email")
    export GIT_SSH_COMMAND="ssh -o IdentitiesOnly=no -o StrictHostKeyChecking=yes -o UserKnownHostsFile=$state/git-known-hosts"

    if test -d "$workspace/.git" &&
        test "$(git -C "$workspace" remote get-url origin)" = "$source" &&
        git -C "$workspace" rev-parse --verify HEAD >/dev/null 2>&1; then
        :
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
    unset GIT_SSH_COMMAND
    rm -f "$state/source" "$state/git-branch" "$state/git-ref" \
        "$state/git-user-name" "$state/git-user-email" "$state/git-known-hosts"
fi

sudo /usr/local/libexec/wt-setup-root prepare

additional_features='{"ghcr.io/devcontainers/features/sshd:1":{}}'
devcontainer up --log-level debug --log-format text --workspace-folder "$workspace" \
    --additional-features "$additional_features" \
    --mount type=bind,source=/var/lib/wt-app-ssh/public,target=/run/wt-app-ssh \
    --mount type=bind,source=/var/lib/wt-app-ssh/public/sshd_config,target=/etc/ssh/sshd_config
devcontainer exec --workspace-folder "$workspace" /bin/sh -c \
    'workspace=$(pwd -P) && git config --global --add safe.directory "$workspace"'
/usr/local/bin/wt-app-info > "$state/app.json"
app_user=$(/usr/local/bin/wt-app-info user)
app_address=$(/usr/local/bin/wt-app-info address)
cat "$state/authorized-keys" /var/lib/wt-app-ssh/session_identity.pub > "$state/app-authorized-keys"
sudo /usr/local/libexec/wt-setup-root finalize "$app_user"
ssh-keyscan -T 5 -p 2222 "$app_address" > "$state/app-keyscan"
expected=$(awk '{print $1 " " $2}' /var/lib/wt-app-ssh/public/ssh_host_ed25519_key.pub)
scanned=$(awk '$2 == "ssh-ed25519" {print $2 " " $3}' "$state/app-keyscan")
test "$scanned" = "$expected"
printf 'wt-app %s\n' "$expected" > /var/lib/wt-app-ssh/known_hosts
ssh -p 2222 -i /var/lib/wt-app-ssh/session_identity -o BatchMode=yes \
    -o IdentitiesOnly=yes -o UserKnownHostsFile=/var/lib/wt-app-ssh/known_hosts \
    -o StrictHostKeyChecking=yes -o HostKeyAlias=wt-app \
    "$app_user@$app_address" true

sudo /usr/local/libexec/wt-setup-root cleanup
echo "World setup complete. Entering the devcontainer."
"$inner" && exit 0
finish_log 0
exec /usr/local/bin/wt-app-pane
