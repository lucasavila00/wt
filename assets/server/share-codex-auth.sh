#!/bin/sh
set -eu

codex_home=/home/wt/.codex
auth=$codex_home/auth.json
share=$codex_home/.wt-auth
temporary=$share/auth.json.wt-new
shared_auth=$share/auth.json

if test -L "$auth" || ! test -f "$auth"; then
    echo "Codex authentication must be a regular, non-symlink file: $auth" >&2
    exit 1
fi
if test "$(stat -c %u "$auth")" != "$(id -u)"; then
    echo "Codex authentication must be owned by the WT server user: $auth" >&2
    exit 1
fi

install -d -m 0700 "$share"
setfacl -m u:1001:rx,m::rx "$share"
auth_acl=$(getfacl -cp "$auth")
printf '%s\n' "$auth_acl" | grep -Fqx 'user:1001:r--' ||
    setfacl -m u:1001:r,m::r "$auth"
rm -f "$temporary"
if test ! -L "$shared_auth" && test -f "$shared_auth" &&
    test "$(stat -c %d:%i "$auth")" = "$(stat -c %d:%i "$shared_auth")"; then
    exit 0
fi
ln "$auth" "$temporary"
mv -f "$temporary" "$shared_auth"
test "$(stat -c %d:%i "$auth")" = "$(stat -c %d:%i "$shared_auth")"
