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
    source=$(cat "$state/source")
    base=$(cat "$state/git-base")
    prefix=$(cat "$state/git-prefix")
    git_name=$(cat "$state/git-user-name")
    git_email=$(cat "$state/git-user-email")
    origin="ag::$source"

    if test -d "$workspace/.git" &&
        test "$(git -C "$workspace" remote get-url origin)" = "$origin" &&
        git -C "$workspace" rev-parse --verify HEAD >/dev/null 2>&1; then
        :
    else
        find "$workspace" -mindepth 1 -maxdepth 1 -exec rm -rf -- {} +
        git clone "$origin" "$workspace"
    fi
    git -C "$workspace" fetch origin "$base"
    git -C "$workspace" checkout -B "$base" "origin/$base"
    git -C "$workspace" config user.name "$git_name"
    git -C "$workspace" config user.email "$git_email"
    git -C "$workspace" config push.autoSetupRemote true
    git -C "$workspace" config wt.project "$source"
    git -C "$workspace" config wt.base "$base"
    git -C "$workspace" config wt.prefix "$prefix"
    for spec in "post-checkout checkout" "post-commit commit"; do
        hook_name=${spec%% *}
        hint_mode=${spec#* }
        hook="$workspace/.git/hooks/$hook_name"
        project_hook="$hook.wt-project"
        if test -e "$hook" && ! grep -q '^# WT agent Git hint$' "$hook"; then
            mv "$hook" "$project_hook"
        fi
        cat > "$hook" <<EOF
#!/bin/sh
# WT agent Git hint
status=0
test ! -x '$project_hook' || '$project_hook' "\$@" || status=\$?
/usr/local/bin/wt-agent-git-hint '$hint_mode'
exit "\$status"
EOF
        chmod 0755 "$hook"
    done
    rm -f "$state/source" "$state/git-base" "$state/git-prefix" \
        "$state/git-user-name" "$state/git-user-email"
fi

sudo /usr/local/libexec/wt-setup-root prepare

additional_features='{"ghcr.io/devcontainers/features/sshd:1":{}}'
app_user=$(
    devcontainer read-configuration --workspace-folder "$workspace" |
        /usr/local/bin/wt-app-info configured-user
)
devcontainer up --log-level debug --log-format text --workspace-folder "$workspace" \
    --additional-features "$additional_features" \
    --mount type=bind,source=/var/lib/wt-app-ssh/public,target=/run/wt-app-ssh \
    --mount type=bind,source=/var/lib/wt-app-ssh/public/sshd_config,target=/etc/ssh/sshd_config \
    --mount type=bind,source=/run/wt-agent-git,target=/run/wt-agent-git \
    --mount type=bind,source=/usr/local/bin/git-remote-ag,target=/usr/local/bin/git-remote-ag \
    --mount type=bind,source=/usr/local/bin/ag-git,target=/usr/local/bin/ag-git \
    --mount type=bind,source=/usr/local/bin/wt-agent-git-hint,target=/usr/local/bin/wt-agent-git-hint
devcontainer exec --workspace-folder "$workspace" /bin/sh -c \
    'workspace=$(pwd -P) && git config --global --add safe.directory "$workspace"'
/usr/local/bin/wt-app-info verify-user "$app_user"
/usr/local/bin/wt-app-info > "$state/app.json"
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
/usr/bin/tmux set-option -g remain-on-exit off
echo "World setup complete. Entering the devcontainer."
"$inner" && exit 0
finish_log 0
exec /usr/local/bin/wt-app-pane
