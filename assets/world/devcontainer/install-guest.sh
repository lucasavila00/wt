#!/bin/sh
set -eu

. /usr/local/share/wt-retained-contract

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

test "$(id -u "$WT_USER")" = "$WT_UID" && test "$(id -g "$WT_USER")" = "$WT_GID" || {
    echo "image user $WT_USER must use uid=$WT_UID and gid=$WT_GID" >&2
    exit 1
}
usermod -aG docker "$WT_USER"
install -d -m 0755 -o "$WT_USER" -g "$WT_USER" /workspace
install -d -m 0755 -o "$WT_USER" -g "$WT_USER" "$WT_HOME/.byobu"
printf '%s\n' \
    'source-file /usr/local/share/wt-tmux.conf' \
    'set-option -g default-command /usr/local/bin/wt-app-pane' \
    > "$WT_HOME/.byobu/.tmux.conf"
test -f "$WT_HOME/.byobu/color"
chown "$WT_USER:$WT_USER" "$WT_HOME/.byobu/.tmux.conf" "$WT_HOME/.byobu/color"
chmod 0644 "$WT_HOME/.byobu/.tmux.conf" "$WT_HOME/.byobu/color"

install -m 0755 "$stage-app-shell" /usr/local/bin/wt-app-shell
install -m 0755 "$stage-setup-world" /usr/local/bin/wt-setup-world
install -d -m 0755 /usr/local/libexec
install -m 0755 "$stage-setup-world-root" /usr/local/libexec/wt-setup-root
install -m 0755 "$stage-app-pane" /usr/local/bin/wt-app-pane
install -m 0755 "$stage-app-info" /usr/local/bin/wt-app-info
install -m 0755 "$stage-app-proxy" /usr/local/bin/wt-app-proxy
install -m 0755 "$stage-agent-git-hint" /usr/local/bin/wt-agent-git-hint
install -d -m 0755 -o "$WT_USER" -g "$WT_USER" /var/lib/wt-setup
install -m 0644 "$WT_HOME/.byobu/.tmux.conf" \
    /usr/local/share/wt-devcontainer-tmux.conf
printf '%s\n' "$deferred_packages" > /var/lib/wt-setup/deferred-packages
printf '%s\n' "$devcontainer_version" > /var/lib/wt-setup/devcontainer-version
printf '%s\n' "$registry_url" > /var/lib/wt-setup/registry-url
install -m 0600 -o "$WT_USER" -g "$WT_USER" "$stage-registry-ca" /var/lib/wt-setup/registry-ca
chown "$WT_USER:$WT_USER" /var/lib/wt-setup/deferred-packages /var/lib/wt-setup/devcontainer-version \
    /var/lib/wt-setup/registry-url
chmod 0600 /var/lib/wt-setup/deferred-packages /var/lib/wt-setup/devcontainer-version \
    /var/lib/wt-setup/registry-url
for name in source git-base git-prefix git-user-name git-user-email; do
    install -m 0600 -o "$WT_USER" -g "$WT_USER" "/tmp/wt-setup-$name" "/var/lib/wt-setup/$name"
    rm -f "/tmp/wt-setup-$name"
done
printf '%s ALL=(root) NOPASSWD: /usr/local/libexec/wt-setup-root *\n' "$WT_USER" > /etc/sudoers.d/wt-setup
chmod 0440 /etc/sudoers.d/wt-setup
