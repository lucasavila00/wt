#!/bin/sh
set -eu

codex_home=/home/wt/.codex
auth=$codex_home/auth.json
share=$codex_home/.wt-auth
temporary=$share/auth.json.wt-new
shared_auth=$share/auth.json
server_uid=$(id -u wt)

if test -L "$auth" || ! test -f "$auth"; then
    echo "Codex authentication must be a regular, non-symlink file: $auth" >&2
    exit 1
fi
if test "$(stat -c %u "$auth")" != "$server_uid"; then
    echo "Codex authentication must be owned by the WT server user: $auth" >&2
    exit 1
fi

if test "$(id -u)" != 0; then
    exec sudo /bin/sh "$0"
fi

install -d -m 0750 -o "$server_uid" -g 1001 "$share"
chown "$server_uid:1001" "$auth"
chmod 0640 "$auth"
rm -f "$temporary"
if test ! -L "$shared_auth" && test -f "$shared_auth" &&
    test "$(stat -c %d:%i "$auth")" = "$(stat -c %d:%i "$shared_auth")"; then
    exit 0
fi
ln "$auth" "$temporary"
mv -f "$temporary" "$shared_auth"
test "$(stat -c %d:%i "$auth")" = "$(stat -c %d:%i "$shared_auth")"
