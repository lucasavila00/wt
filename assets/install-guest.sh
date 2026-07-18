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

id wt >/dev/null 2>&1 || useradd --create-home --shell /bin/bash wt
usermod -aG docker wt
install -d -m 0755 -o wt -g wt /workspace
install -d -m 0700 -o wt -g wt /home/wt/.ssh
install -o wt -g wt -m 0600 "$stage-authorized-keys" /home/wt/.ssh/authorized_keys
ssh-keygen -A

install -m 0755 "$stage-app-shell" /usr/local/bin/wt-app-shell
install -m 0755 "$stage-setup-world" /usr/local/bin/wt-setup-world
install -d -m 0755 /usr/local/libexec
install -m 0755 "$stage-setup-world-root" /usr/local/libexec/wt-setup-root
install -m 0755 "$stage-app-pane" /usr/local/bin/wt-app-pane
install -m 0755 "$stage-app-info" /usr/local/bin/wt-app-info
install -m 0755 "$stage-app-proxy" /usr/local/bin/wt-app-proxy
printf '%s\n' \
    'set-option -g default-command /usr/local/bin/wt-app-pane' \
    'set-option -g remain-on-exit failed' \
    'set-option -s set-clipboard on' \
    'set-option -g allow-passthrough on' \
    'set-option -g focus-events on' \
    "set-option -as terminal-features ',xterm-ghostty:clipboard'" \
    > /usr/local/share/wt-tmux.conf
chmod 0644 /usr/local/share/wt-tmux.conf
install -d -m 0755 -o wt -g wt /var/lib/wt-setup
printf '%s\n' "$deferred_packages" > /var/lib/wt-setup/deferred-packages
printf '%s\n' "$devcontainer_version" > /var/lib/wt-setup/devcontainer-version
printf '%s\n' "$registry_url" > /var/lib/wt-setup/registry-url
install -m 0600 -o wt -g wt "$stage-registry-ca" /var/lib/wt-setup/registry-ca
chown wt:wt /var/lib/wt-setup/deferred-packages /var/lib/wt-setup/devcontainer-version \
    /var/lib/wt-setup/registry-url
chmod 0600 /var/lib/wt-setup/deferred-packages /var/lib/wt-setup/devcontainer-version \
    /var/lib/wt-setup/registry-url
install -m 0600 -o wt -g wt "$stage-authorized-keys" /var/lib/wt-setup/authorized-keys
for name in source git-branch git-ref git-user-name git-user-email git-known-hosts; do
    install -m 0600 -o wt -g wt "/tmp/wt-setup-$name" "/var/lib/wt-setup/$name"
    rm -f "/tmp/wt-setup-$name"
done
printf 'wt ALL=(root) NOPASSWD: /usr/local/libexec/wt-setup-root *\n' > /etc/sudoers.d/wt-setup
chmod 0440 /etc/sudoers.d/wt-setup

systemctl enable --now ssh.service
