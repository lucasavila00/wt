#!/bin/sh
set -eu

state=/var/lib/wt-setup
tmux=/usr/bin/tmux
byobu=/usr/bin/byobu-tmux
exec 9>"$state/app-shell.lock"
flock 9
unset SSH_AUTH_SOCK
if test -n "${BYOBU_ALT_TITLE:-}" && test -e "$state/complete"; then
    checkout=$(
        /usr/bin/git -C /workspace symbolic-ref --quiet --short HEAD 2>/dev/null ||
            /usr/bin/git -C /workspace rev-parse --short HEAD 2>/dev/null ||
            true
    )
    case "$checkout" in
        ""|*[!A-Za-z0-9./_-]*) ;;
        *) BYOBU_ALT_TITLE="$BYOBU_ALT_TITLE@$checkout"; export BYOBU_ALT_TITLE ;;
    esac
fi
if ! "$tmux" has-session -t wt-app 2>/dev/null; then
    attempt=1
    while ! "$byobu" -f /usr/local/share/wt-tmux.conf new-session -d -s wt-app \
        "$(test -e "$state/complete" && echo /usr/local/bin/wt-app-pane || echo /usr/local/bin/wt-setup-world)"; do
        "$tmux" has-session -t wt-app 2>/dev/null && break
        if test "$attempt" -ge 3; then
            echo "wt: failed to start the Byobu session after $attempt attempts" >&2
            exit 1
        fi
        echo "wt: failed to start the Byobu session on attempt $attempt; retrying" >&2
        attempt=$((attempt + 1))
        sleep 1
    done
fi
"$tmux" set-option -g remain-on-exit failed
if test -e "$state/complete"; then
    if test "$("$tmux" display-message -p -t wt-app:0.0 '#{pane_dead}')" = 1; then
        "$tmux" respawn-pane -k -t wt-app:0.0 /usr/local/bin/wt-app-pane
    fi
else
    if test "$("$tmux" display-message -p -t wt-app:0.0 '#{pane_dead}')" = 1; then
        "$tmux" respawn-pane -k -t wt-app:0.0 /usr/local/bin/wt-setup-world
    fi
fi
flock -u 9
exec "$tmux" attach-session -t wt-app
