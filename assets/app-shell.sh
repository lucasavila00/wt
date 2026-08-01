#!/bin/sh
set -eu

state=/var/lib/wt-setup
tmux=/usr/bin/tmux
agent_socket=$state/ssh-agent.sock
exec 9>"$state/app-shell.lock"
flock 9
if test -n "${SSH_AUTH_SOCK:-}"; then
    ln -sfn "$SSH_AUTH_SOCK" "$agent_socket"
    SSH_AUTH_SOCK=$agent_socket
    export SSH_AUTH_SOCK
else
    rm -f "$agent_socket"
    unset SSH_AUTH_SOCK
fi
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
    "$tmux" -f /usr/local/share/wt-tmux.conf new-session -d -s wt-app \
        "$(test -e "$state/complete" && echo /usr/local/bin/wt-app-pane || echo /usr/local/bin/wt-setup-world)"
else
    if test -n "${SSH_AUTH_SOCK:-}"; then
        "$tmux" set-environment -t wt-app SSH_AUTH_SOCK "$SSH_AUTH_SOCK"
    else
        "$tmux" set-environment -u -t wt-app SSH_AUTH_SOCK
    fi
fi
if test -e "$state/complete"; then
    if test "$("$tmux" display-message -p -t wt-app:0.0 '#{pane_dead}')" = 1; then
        "$tmux" respawn-pane -k -t wt-app:0.0 /usr/local/bin/wt-app-pane
    fi
    "$tmux" set-option -g remain-on-exit off
else
    "$tmux" set-option -g remain-on-exit failed
    if test "$("$tmux" display-message -p -t wt-app:0.0 '#{pane_dead}')" = 1; then
        "$tmux" respawn-pane -k -t wt-app:0.0 /usr/local/bin/wt-setup-world
    fi
fi
flock -u 9
exec "$tmux" attach-session -t wt-app
