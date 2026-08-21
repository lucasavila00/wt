#!/bin/sh
set -eu

shutdown() {
    status=$?
    trap - EXIT
    sync
    systemctl poweroff || true
    exit "$status"
}
trap shutdown EXIT

. /var/tmp/wt-image-build.env

phase() {
    echo "WT_IMAGE_PHASE=$*" > /dev/ttyS0
}

phase "installing base operating-system packages"
/bin/sh /var/tmp/wt-install-packages.sh \
    openssh-server qemu-guest-agent tmux \
    bison build-essential curl libevent-dev libncurses-dev pkg-config

phase "configuring base operating-system services"
systemctl enable --now qemu-guest-agent.service
systemctl disable --now ssh.service ssh.socket
if ! getent group "$WT_USER" >/dev/null; then
    groupadd --gid "$WT_GID" "$WT_USER"
fi
if ! id "$WT_USER" >/dev/null 2>&1; then
    useradd --uid "$WT_UID" --gid "$WT_GID" --create-home \
        --home-dir "$WT_HOME" --shell /bin/bash "$WT_USER"
fi
test "$(id -u "$WT_USER")" = "$WT_UID"
test "$(id -g "$WT_USER")" = "$WT_GID"
test "$(getent passwd "$WT_USER" | cut -d: -f6)" = "$WT_HOME"
printf 'kernel.perf_event_paranoid = -1\n' > /etc/sysctl.d/99-wt-profiling.conf
sysctl --system
test "$(cat /proc/sys/kernel/perf_event_paranoid)" = -1

phase "installing terminal tools"
/bin/sh /var/tmp/wt-install-terminal.sh

phase "installing Codex"
/bin/sh /var/tmp/wt-install-codex.sh
install -d -m 0755 /usr/local/share /usr/local/libexec
printf "WT_USER='%s'\nWT_UID='%s'\nWT_GID='%s'\nWT_HOME='%s'\n" \
    "$WT_USER" "$WT_UID" "$WT_GID" "$WT_HOME" \
    > /usr/local/share/wt-retained-contract
chmod 0644 /usr/local/share/wt-retained-contract
install -m 0644 /var/tmp/wt-tmux.conf /usr/local/share/wt-tmux.conf
install -m 0755 /var/tmp/wt-retained-access /usr/local/libexec/wt-retained-access
install -m 0755 /var/tmp/wt-retained-git-author \
    /usr/local/libexec/wt-retained-git-author
install -m 0755 /var/tmp/wt-retained-agent-tools /usr/local/libexec/wt-retained-agent-tools
install -m 0755 /var/tmp/wt-retained-mount-codex \
    /usr/local/libexec/wt-retained-mount-codex
printf '%s  %s\n' "$TMUX_CONFIG_SHA256" \
    /usr/local/share/wt-tmux.conf | sha256sum --check --strict
printf '%s  %s\n' "$ACCESS_SHA256" \
    /usr/local/libexec/wt-retained-access | sha256sum --check --strict
printf '%s  %s\n' "$GIT_AUTHOR_SHA256" \
    /usr/local/libexec/wt-retained-git-author | sha256sum --check --strict
printf '%s  %s\n' "$AGENT_TOOLS_SHA256" \
    /usr/local/libexec/wt-retained-agent-tools | sha256sum --check --strict
printf '%s  %s\n' "$MOUNT_CODEX_SHA256" \
    /usr/local/libexec/wt-retained-mount-codex | sha256sum --check --strict

phase "installing retained-world tools"
/bin/sh /var/tmp/wt-retained-image-build.sh

phase "removing image-build dependencies"
DEBIAN_FRONTEND=noninteractive apt-get autoremove --purge -y \
    bison build-essential curl libevent-dev libncurses-dev pkg-config
DEBIAN_FRONTEND=noninteractive apt-get purge -y \
    cloud-init cloud-initramfs-copymods cloud-initramfs-dyn-netconf
DEBIAN_FRONTEND=noninteractive apt-get autoremove --purge -y
apt-get clean
! command -v cc
! command -v gcc
! command -v g++
! command -v make
! command -v curl
! command -v cloud-init

phase "validating installed terminal tools"
test "$(/usr/bin/tmux -V)" = "tmux $TMUX_VERSION"
test "$(dpkg-query -W -f='${Version}' byobu)" = "$BYOBU_VERSION"
printf '%s  %s\n' "$GHOSTTY_TERMINFO_SHA256" \
    /usr/share/terminfo/g/ghostty | sha256sum --check --strict
cmp /usr/share/terminfo/g/ghostty /usr/share/terminfo/x/xterm-ghostty
TERM=ghostty tput colors >/dev/null
TERM=xterm-ghostty tput colors >/dev/null

rm -f /var/tmp/wt-*.sh /var/tmp/wt-image-build.env \
    /var/tmp/wt-tmux.conf /var/tmp/wt-byobu-color /var/tmp/wt-host-*
printf 'kind=%s\nstatus=ready\nwt_uid=%s\nwt_gid=%s\n' \
    "$WT_IMAGE_KIND" "$WT_UID" "$WT_GID" \
    > /var/lib/wt-image-result
chown root:root /var/lib/wt-image-result
chmod 0644 /var/lib/wt-image-result
phase "recipe complete; requesting VM shutdown"
