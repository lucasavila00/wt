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
        sudoers=/run/wt-host-sudoers.wt-new
        rm -f "$sudoers"
        printf 'wt ALL=(ALL:ALL) NOPASSWD: ALL\n' > "$sudoers"
        chown root:root "$sudoers"
        chmod 0440 "$sudoers"
        visudo --check --file="$sudoers" >/dev/null
        mv -f "$sudoers" /etc/sudoers.d/wt
        if ! test -s /etc/sudoers.d/wt; then
            echo "WT sudoers rule is empty after installation" >&2
            exit 1
        fi
        runuser --user wt -- sudo --non-interactive true
        ssh-keygen -A
        if ! systemctl enable --now ssh.service; then
            service_diagnostics ssh.service
            exit 1
        fi
        ;;
    agent-git)
        vsock_port=$(cat /tmp/wt-host-agent-git-vsock-port)
        case "$vsock_port" in
            ''|*[!0-9]*) echo "invalid agent Git vsock port" >&2; exit 1 ;;
        esac
        install -m 0755 /tmp/wt-host-agent-git-relay /usr/local/bin/wt-agent-git-relay
        install -m 0755 /tmp/wt-host-agent-git-remote /usr/local/bin/git-remote-ag
        install -m 0755 /tmp/wt-host-ag-git /usr/local/bin/ag-git
        install -d -m 0700 -o wt -g wt /var/lib/wt-agent-git
        install -m 0600 -o wt -g wt /tmp/wt-host-agent-git-grant \
            /var/lib/wt-agent-git/grant
        while IFS= read -r host; do
            test -n "$host" || continue
            runuser --user wt -- git config --global --replace-all \
                "url.ag::git@$host:.insteadOf" "git@$host:"
            runuser --user wt -- git config --global --add \
                "url.ag::git@$host:.insteadOf" "ssh://git@$host/"
            runuser --user wt -- git config --global --add \
                "url.ag::git@$host:.insteadOf" "https://$host/"
        done < /tmp/wt-host-agent-git-providers
        cat > /etc/systemd/system/wt-agent-git-relay.service <<EOF
[Unit]
Description=WT agent Git relay

[Service]
Type=simple
User=wt
ExecStart=/usr/local/bin/wt-agent-git-relay --vsock-port $vsock_port
Restart=on-failure
RuntimeDirectory=wt-agent-git
RuntimeDirectoryMode=0755
RuntimeDirectoryPreserve=restart
UMask=0077

[Install]
WantedBy=multi-user.target
EOF
        rm -f /tmp/wt-host-agent-git-grant /tmp/wt-host-agent-git-relay \
            /tmp/wt-host-agent-git-remote /tmp/wt-host-ag-git \
            /tmp/wt-host-agent-git-providers /tmp/wt-host-agent-git-vsock-port
        systemctl daemon-reload
        if ! systemctl enable --now wt-agent-git-relay.service; then
            service_diagnostics wt-agent-git-relay.service
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
        sync
        ;;
    *)
        echo "usage: wt-host-prepare wait|access|agent-git|user-data|remove-key" >&2
        exit 2
        ;;
esac
