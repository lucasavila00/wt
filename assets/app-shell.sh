#!/bin/sh
set -eu

state=/var/lib/wt-setup
tmux=/usr/bin/byobu-tmux
if ! "$tmux" has-session -t wt-app 2>/dev/null; then
    "$tmux" -f /usr/local/share/wt-tmux.conf new-session -d -s wt-app \
        "$(test -e "$state/complete" && echo /usr/local/bin/wt-app-pane || echo /usr/local/bin/wt-setup-world)"
else
  if test -n "${SSH_AUTH_SOCK:-}"; then
    "$tmux" set-environment -t wt-app SSH_AUTH_SOCK "$SSH_AUTH_SOCK"
  fi
  if test -e "$state/complete"; then
    "$tmux" new-window -t wt-app /usr/local/bin/wt-app-pane
  else
    "$tmux" new-window -t wt-app /usr/local/bin/wt-setup-world
  fi
fi
exec "$tmux" attach-session -t wt-app
