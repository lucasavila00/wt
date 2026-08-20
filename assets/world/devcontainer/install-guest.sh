#!/bin/sh
set -eu

if test "$#" -lt 3; then
    echo "usage: install-guest.sh DEVCONTAINER_VERSION REGISTRY_URL PACKAGE..." >&2
    exit 2
fi

devcontainer_version=$1
registry_url=$2
shift 2

stage=/tmp/wt-install-guest
export DEBIAN_FRONTEND=noninteractive
minimal_packages=
deferred_packages=
for package in "$@"; do
    case "${package%%=*}" in
        ca-certificates|git|openssh-server|byobu|tmux) minimal_packages="$minimal_packages $package" ;;
        *) deferred_packages="$deferred_packages $package" ;;
    esac
done
attempt=0
until apt-get update && apt-get install -y --no-install-recommends $minimal_packages; do
    attempt=$((attempt + 1))
    test "$attempt" -lt 30 || exit 1
    sleep 2
done

if ! id wt >/dev/null 2>&1; then
    getent group wt >/dev/null || groupadd --gid 1001 wt
    useradd --uid 1001 --gid 1001 --create-home --shell /bin/bash wt
fi
test "$(id -u wt)" = 1001 && test "$(id -g wt)" = 1001 || {
    echo 'wt must use uid=1001 and gid=1001' >&2
    exit 1
}
usermod -aG docker wt
install -d -m 0755 -o wt -g wt /workspace
install -d -m 0700 -o wt -g wt /home/wt/.ssh
install -o wt -g wt -m 0600 "$stage-authorized-keys" /home/wt/.ssh/authorized_keys
install -d -m 0755 -o wt -g wt /home/wt/.byobu
printf '%s\n' \
    'source-file /usr/local/share/wt-tmux.conf' \
    'set-option -g default-command /usr/local/bin/wt-app-pane' \
    > /home/wt/.byobu/.tmux.conf
test -f /home/wt/.byobu/color
chown wt:wt /home/wt/.byobu/.tmux.conf /home/wt/.byobu/color
chmod 0644 /home/wt/.byobu/.tmux.conf /home/wt/.byobu/color
ssh-keygen -A

install -m 0755 "$stage-app-shell" /usr/local/bin/wt-app-shell
install -m 0755 "$stage-setup-world" /usr/local/bin/wt-setup-world
install -d -m 0755 /usr/local/libexec
install -m 0755 "$stage-setup-world-root" /usr/local/libexec/wt-setup-root
install -m 0755 "$stage-app-pane" /usr/local/bin/wt-app-pane
install -m 0755 "$stage-app-info" /usr/local/bin/wt-app-info
install -m 0755 "$stage-app-proxy" /usr/local/bin/wt-app-proxy
install -m 0755 "$stage-agent-git-relay" /usr/local/bin/wt-agent-git-relay
install -m 0755 "$stage-agent-git-remote" /usr/local/bin/git-remote-ag
install -m 0755 "$stage-ag-git" /usr/local/bin/ag-git
install -m 0755 "$stage-agent-git-hint" /usr/local/bin/wt-agent-git-hint
install -d -m 0755 -o wt -g wt /var/lib/wt-setup
install -m 0600 -o wt -g wt "$stage-agent-git-providers" \
    /var/lib/wt-setup/agent-git-providers
while IFS= read -r host; do
    test -n "$host" || continue
    runuser --user wt -- git config --global --replace-all \
        "url.ag::git@$host:.insteadOf" "git@$host:"
    runuser --user wt -- git config --global --add \
        "url.ag::git@$host:.insteadOf" "ssh://git@$host/"
    runuser --user wt -- git config --global --add \
        "url.ag::git@$host:.insteadOf" "https://$host/"
done < /var/lib/wt-setup/agent-git-providers
install -m 0644 /home/wt/.byobu/.tmux.conf \
    /usr/local/share/wt-devcontainer-tmux.conf
install -d -m 0700 -o wt -g wt /var/lib/wt-agent-git
install -m 0600 -o wt -g wt /tmp/wt-setup-git-grant /var/lib/wt-agent-git/grant
rm -f /tmp/wt-setup-git-grant
vsock_port=$(cat "$stage-agent-git-vsock-port")
case "$vsock_port" in
    ''|*[!0-9]*) echo "invalid agent Git vsock port" >&2; exit 1 ;;
esac
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
printf '%s\n' "$deferred_packages" > /var/lib/wt-setup/deferred-packages
printf '%s\n' "$devcontainer_version" > /var/lib/wt-setup/devcontainer-version
printf '%s\n' "$registry_url" > /var/lib/wt-setup/registry-url
install -m 0600 -o wt -g wt "$stage-registry-ca" /var/lib/wt-setup/registry-ca
chown wt:wt /var/lib/wt-setup/deferred-packages /var/lib/wt-setup/devcontainer-version \
    /var/lib/wt-setup/registry-url
chmod 0600 /var/lib/wt-setup/deferred-packages /var/lib/wt-setup/devcontainer-version \
    /var/lib/wt-setup/registry-url
install -m 0600 -o wt -g wt "$stage-authorized-keys" /var/lib/wt-setup/authorized-keys
for name in source git-base git-prefix git-user-name git-user-email; do
    install -m 0600 -o wt -g wt "/tmp/wt-setup-$name" "/var/lib/wt-setup/$name"
    rm -f "/tmp/wt-setup-$name"
done
printf 'wt ALL=(root) NOPASSWD: /usr/local/libexec/wt-setup-root *\n' > /etc/sudoers.d/wt-setup
chmod 0440 /etc/sudoers.d/wt-setup

systemctl daemon-reload
systemctl enable --now wt-agent-git-relay.service ssh.service
