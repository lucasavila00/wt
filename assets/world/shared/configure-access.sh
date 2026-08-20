#!/bin/sh
set -eu

. /usr/local/share/wt-retained-contract

test "$(id -u "$WT_USER")" = "$WT_UID"
test "$(id -g "$WT_USER")" = "$WT_GID"

install -d -m 0700 -o "$WT_USER" -g "$WT_USER" "$WT_HOME/.ssh"
temporary=$WT_HOME/.ssh/authorized_keys.wt-new
cat > "$temporary"
chown "$WT_USER:$WT_USER" "$temporary"
chmod 0600 "$temporary"
mv -f "$temporary" "$WT_HOME/.ssh/authorized_keys"
ssh-keygen -A

if ! systemctl enable --now ssh.service; then
    systemctl status --no-pager --full ssh.service >&2 || true
    journalctl --no-pager -u ssh.service -n 100 >&2 || true
    exit 1
fi
