#!/bin/sh
set -eu

. /usr/local/share/wt-retained-contract

test "$(id -u "$WT_USER")" = "$WT_UID"
test "$(id -g "$WT_USER")" = "$WT_GID"

install -d -m 0700 -o "$WT_USER" -g "$WT_GROUP" "$WT_HOME/.ssh"
temporary=$WT_HOME/.ssh/authorized_keys.wt-new
cat > "$temporary"
chown "$WT_USER:$WT_GROUP" "$temporary"
chmod 0600 "$temporary"
mv -f "$temporary" "$WT_HOME/.ssh/authorized_keys"
install -d -m 0755 -o root -g root /etc/ssh/sshd_config.d
printf 'AuthorizedKeysFile .ssh/authorized_keys /run/wt-ssh-authorized-keys/authorized_keys\n' \
    > /etc/ssh/sshd_config.d/50-wt-authorized-keys.conf
chmod 0644 /etc/ssh/sshd_config.d/50-wt-authorized-keys.conf
ssh-keygen -A

if ! systemctl enable --now ssh.service; then
    systemctl status --no-pager --full ssh.service >&2 || true
    journalctl --no-pager -u ssh.service -n 100 >&2 || true
    exit 1
fi
