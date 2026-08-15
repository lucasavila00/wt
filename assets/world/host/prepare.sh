#!/bin/sh
set -eu

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
    access)
        if ! id wt >/dev/null 2>&1; then
            useradd --create-home --shell /bin/bash wt
        fi
        usermod --append --groups sudo wt
        install -d -m 0700 -o wt -g wt /home/wt/.ssh
        temporary=/home/wt/.ssh/authorized_keys.wt-new
        cat > "$temporary"
        chown wt:wt "$temporary"
        chmod 0600 "$temporary"
        mv -f "$temporary" /home/wt/.ssh/authorized_keys
        printf 'wt ALL=(ALL) NOPASSWD:ALL\n' > /etc/sudoers.d/wt
        chmod 0440 /etc/sudoers.d/wt
        visudo --check --file=/etc/sudoers.d/wt >/dev/null
        ssh-keygen -A
        if ! systemctl enable --now ssh.service; then
            service_diagnostics ssh.service
            exit 1
        fi
        ;;
    user-data)
        install -d -m 0700 -o root -g root "$state"
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
        file=/home/wt/.ssh/authorized_keys
        temporary=$file.wt-readiness
        grep -Fvx -- "$key" "$file" > "$temporary"
        chown wt:wt "$temporary"
        chmod 0600 "$temporary"
        mv -f "$temporary" "$file"
        ;;
    *)
        echo "usage: wt-host-prepare wait|access|user-data|remove-key" >&2
        exit 2
        ;;
esac
