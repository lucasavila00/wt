#!/bin/sh
set -eu
umask 077

tmux=/usr/bin/tmux
byobu=/usr/bin/byobu-tmux
state=${XDG_STATE_HOME:-"$HOME/.local/state"}/wt
mkdir -p "$state"
exec 9>"$state/host-shell.lock"
flock 9
agent="$state/ssh-agent"
if test -n "${SSH_AUTH_SOCK:-}" && test -S "$SSH_AUTH_SOCK"; then
    temporary="$state/.ssh-agent.new"
    rm -f "$temporary"
    ln -s "$SSH_AUTH_SOCK" "$temporary"
    mv -f "$temporary" "$agent"
fi
SSH_AUTH_SOCK=$agent
export SSH_AUTH_SOCK
if ! "$tmux" has-session -t wt-host 2>/dev/null; then
    attempt=1
    command=/bin/bash
    test -e /var/lib/wt-host/complete || command=/usr/local/bin/wt-host-setup
    while ! "$byobu" -f /usr/local/share/wt-tmux.conf new-session -d -s wt-host "$command"; do
        "$tmux" has-session -t wt-host 2>/dev/null && break
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
flock -u 9
exec "$tmux" attach-session -t wt-host
