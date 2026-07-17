#!/bin/sh
set -eu

state=/var/lib/wt-setup
tmux=/usr/bin/byobu-tmux
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
    "$tmux" set-option -g remain-on-exit off
else
    "$tmux" set-option -g remain-on-exit failed
fi
exec "$tmux" attach-session -t wt-app
