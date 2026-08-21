#!/bin/sh
set -eu

codex_home=/home/wt/.codex
auth=$codex_home/auth.json
share=$codex_home/.wt-auth
temporary=$codex_home/.wt-auth.wt-new.$$
shared_auth=$share/auth.json
server_uid=$(id -u wt)

cleanup() {
    rm -f "$temporary"
}
trap cleanup EXIT HUP INT TERM

sudo install -d -m 0750 -o "$server_uid" -g 1001 "$share"
while :; do
    if test -L "$auth" || ! test -f "$auth"; then
        echo "Codex authentication must be a regular, non-symlink file: $auth" >&2
        exit 1
    fi
    if test "$(stat -c %u "$auth")" != "$server_uid"; then
        echo "Codex authentication must be owned by the WT server user: $auth" >&2
        exit 1
    fi

    sudo chown "$server_uid:1001" "$auth"
    sudo chmod 0640 "$auth"
    rm -f "$temporary"
    cp "$auth" "$temporary"
    sudo chown "$server_uid:1001" "$temporary"
    sudo chmod 0640 "$temporary"
    mv -f "$temporary" "$shared_auth"

    if cmp -s "$auth" "$shared_auth"; then
        exit 0
    fi
done
