#!/bin/sh
set -eu

if test "$#" -lt 4; then
    echo "usage: install-guest.sh DEVCONTAINER_VERSION SESSION REGISTRY_URL PACKAGE..." >&2
    exit 2
fi

devcontainer_version=$1
session=$2
registry_url=$3
shift 3

case "$session" in
    tmux|byobu) ;;
    *) echo "unsupported session frontend: $session" >&2; exit 2 ;;
esac

stage=/tmp/wt-install-guest
export DEBIAN_FRONTEND=noninteractive
attempt=0
until apt-get update && apt-get install -y --no-install-recommends "$@"; do
    attempt=$((attempt + 1))
    test "$attempt" -lt 30 || exit 1
    sleep 2
done

if ! command -v devcontainer >/dev/null 2>&1 ||
    ! devcontainer --version | grep -Fx "$devcontainer_version" >/dev/null; then
    npm install --global "@devcontainers/cli@$devcontainer_version"
fi
devcontainer --version

id wt >/dev/null 2>&1 || useradd --create-home --shell /bin/bash wt
usermod -aG docker wt
install -d -m 0755 -o wt -g wt /workspace
install -d -m 0700 -o wt -g wt /home/wt/.ssh
install -o wt -g wt -m 0600 "$stage-authorized-keys" /home/wt/.ssh/authorized_keys
ssh-keygen -A

install -m 0644 "$stage-registry-ca" /usr/local/share/ca-certificates/wt-registry-cache.crt
install -d -m 0755 /etc/systemd/system/docker.service.d
printf '[Service]\nEnvironment="HTTP_PROXY=%s"\nEnvironment="HTTPS_PROXY=%s"\nEnvironment="NO_PROXY=localhost,127.0.0.1"\n' \
    "$registry_url" "$registry_url" \
    > /etc/systemd/system/docker.service.d/wt-registry-cache.conf

install -m 0755 "$stage-app-shell" /usr/local/bin/wt-app-shell
install -m 0755 "$stage-app-pane" /usr/local/bin/wt-app-pane
install -m 0755 "$stage-app-info" /usr/local/bin/wt-app-info
install -m 0755 "$stage-app-proxy" /usr/local/bin/wt-app-proxy
printf 'set-option -g default-command /usr/local/bin/wt-app-pane\n' > /usr/local/share/wt-tmux.conf
printf '%s\n' "$session" > /usr/local/share/wt-session-frontend
chmod 0644 /usr/local/share/wt-tmux.conf /usr/local/share/wt-session-frontend

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
systemctl enable --now docker.service ssh.service
systemctl restart docker.service
docker info >/dev/null
docker buildx version
docker compose version
