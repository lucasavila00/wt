#!/bin/sh
set -eu

state=/var/lib/wt-setup
mode=${1:-}

case "$mode" in
prepare)
    test ! -e "$state/root-prepared" || exit 0
    export DEBIAN_FRONTEND=noninteractive
    packages=$(cat "$state/deferred-packages")
    attempt=0
    until apt-get update && apt-get install -y --no-install-recommends $packages; do
        attempt=$((attempt + 1))
        test "$attempt" -lt 30 || exit 1
        sleep 2
    done
    version=$(cat "$state/devcontainer-version")
    if ! command -v devcontainer >/dev/null 2>&1 ||
        ! devcontainer --version | grep -Fx "$version" >/dev/null; then
        npm install --global "@devcontainers/cli@$version"
    fi
    devcontainer --version

    registry_url=$(cat "$state/registry-url")
    install -m 0644 "$state/registry-ca" /usr/local/share/ca-certificates/wt-registry-cache.crt
    install -d -m 0755 /etc/systemd/system/docker.service.d
    printf '[Service]\nEnvironment="HTTP_PROXY=%s"\nEnvironment="HTTPS_PROXY=%s"\nEnvironment="NO_PROXY=localhost,127.0.0.1"\n' \
        "$registry_url" "$registry_url" \
        > /etc/systemd/system/docker.service.d/wt-registry-cache.conf

    install -d -m 0700 -o wt -g wt /var/lib/wt-app-ssh
    install -d -m 0755 /var/lib/wt-app-ssh/public /var/lib/wt-app-ssh/public/authorized_keys
    test -f /var/lib/wt-app-ssh/public/ssh_host_ed25519_key ||
        ssh-keygen -q -t ed25519 -N '' -f /var/lib/wt-app-ssh/public/ssh_host_ed25519_key
    test -f /var/lib/wt-app-ssh/session_identity ||
        ssh-keygen -q -t ed25519 -N '' -f /var/lib/wt-app-ssh/session_identity
    chown wt:wt /var/lib/wt-app-ssh/session_identity /var/lib/wt-app-ssh/session_identity.pub
    chmod 0600 /var/lib/wt-app-ssh/public/ssh_host_ed25519_key /var/lib/wt-app-ssh/session_identity
    chmod 0644 /var/lib/wt-app-ssh/public/ssh_host_ed25519_key.pub /var/lib/wt-app-ssh/session_identity.pub
    cat > /var/lib/wt-app-ssh/public/sshd_config <<'EOF'
Port 2222
HostKey /run/wt-app-ssh/ssh_host_ed25519_key
PidFile /run/sshd-wt.pid
AuthorizedKeysFile /run/wt-app-ssh/authorized_keys/%u
PasswordAuthentication no
KbdInteractiveAuthentication no
ChallengeResponseAuthentication no
UsePAM yes
PermitRootLogin prohibit-password
AllowTcpForwarding yes
GatewayPorts no
X11Forwarding no
PrintMotd no
StrictModes yes
AcceptEnv LANG LC_*
Subsystem sftp internal-sftp
EOF
    chmod 0644 /var/lib/wt-app-ssh/public/sshd_config
    update-ca-certificates
    systemctl daemon-reload
    systemctl enable --now docker.service
    systemctl restart docker.service
    docker info >/dev/null
    docker buildx version
    docker compose version
    rm -f "$state/deferred-packages" "$state/devcontainer-version" \
        "$state/registry-url" "$state/registry-ca"
    touch "$state/root-prepared"
    ;;
finalize)
    user=${2:-}
    case "$user" in
        ''|*[!A-Za-z0-9_.-]*) echo "invalid app SSH user" >&2; exit 2 ;;
    esac
    install -m 0644 -o root -g root "$state/app-authorized-keys" \
        "/var/lib/wt-app-ssh/public/authorized_keys/$user"
    ;;
cleanup)
    rm -f "$state/authorized-keys" "$state/app-authorized-keys" "$state/app-keyscan" \
        "$state/root-prepared"
    rm -f /etc/sudoers.d/wt-setup
    printf 'complete\n' > "$state/complete.new"
    chown wt:wt "$state/complete.new"
    mv "$state/complete.new" "$state/complete"
    ;;
*)
    echo "usage: wt-setup-root prepare | finalize USER | cleanup" >&2
    exit 2
    ;;
esac
