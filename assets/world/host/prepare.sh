#!/bin/sh
set -eu

. /usr/local/share/wt-retained-contract

state=/var/lib/wt-host

service_diagnostics() {
    systemctl status --no-pager --full "$1" >&2 || true
    journalctl --no-pager -u "$1" -n 100 >&2 || true
}

case "${1:-}" in
    wait)
        if ! systemctl start cloud-init.service; then
            service_diagnostics cloud-init.service
            exit 1
        fi
        if [ "$(systemctl show --property=SubState --value cloud-init.service)" != exited ]; then
            echo "cloud-init.service did not finish its boot stage" >&2
            service_diagnostics cloud-init.service
            exit 1
        fi
        ;;
    access-policy)
        test "$(id -u "$WT_USER")" = "$WT_UID" && test "$(id -g "$WT_USER")" = "$WT_GID" || {
            echo "image user $WT_USER must use uid=$WT_UID and gid=$WT_GID" >&2
            exit 1
        }
        usermod --append --groups sudo "$WT_USER"
        sudoers=/run/wt-host-sudoers.wt-new
        rm -f "$sudoers"
        printf '%s ALL=(ALL:ALL) NOPASSWD: ALL\n' "$WT_USER" > "$sudoers"
        chown root:root "$sudoers"
        chmod 0440 "$sudoers"
        visudo --check --file="$sudoers" >/dev/null
        mv -f "$sudoers" /etc/sudoers.d/wt
        if ! test -s /etc/sudoers.d/wt; then
            echo "WT sudoers rule is empty after installation" >&2
            exit 1
        fi
        runuser --user "$WT_USER" -- sudo --non-interactive true
        ;;
    user-data)
        install -d -m 0711 -o root -g root "$state"
        temporary=$state/user-data.wt-new
        cat > "$temporary"
        chown root:root "$temporary"
        chmod 0600 "$temporary"
        mv -f "$temporary" "$state/user-data"
        : > /var/log/cloud-init-output.log
        chown root:root /var/log/cloud-init-output.log
        chmod 0644 /var/log/cloud-init-output.log
        ;;
    remove-key)
        key=$(cat)
        file=$WT_HOME/.ssh/authorized_keys
        temporary=$file.wt-readiness
        grep -Fvx -- "$key" "$file" > "$temporary"
        chown "$WT_USER:$WT_USER" "$temporary"
        chmod 0600 "$temporary"
        mv -f "$temporary" "$file"
        sync
        ;;
    *)
        echo "usage: wt-host-prepare wait|access-policy|user-data|remove-key" >&2
        exit 2
        ;;
esac
