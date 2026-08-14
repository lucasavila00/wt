#!/bin/sh
set -eu

. /var/tmp/wt-image-build.env

phase() {
    echo "WT_IMAGE_PHASE=$*" > /dev/ttyS0
}

phase "installing shared machine packages"
/bin/sh /var/tmp/wt-install-packages.sh \
    openssh-server qemu-guest-agent tmux \
    bison build-essential curl libevent-dev libncurses-dev pkg-config

phase "configuring shared machine services"
systemctl enable --now qemu-guest-agent.service ssh.service

phase "installing shared terminal stack"
/bin/sh /var/tmp/wt-install-terminal.sh
install -d -m 0755 /usr/local/share /etc/skel/.byobu
install -m 0644 /var/tmp/wt-tmux.conf /usr/local/share/wt-tmux.conf
install -m 0644 /var/tmp/wt-tmux.conf /etc/skel/.byobu/.tmux.conf
install -m 0644 /var/tmp/wt-byobu-color /etc/skel/.byobu/color
printf '%s  %s\n' "$TMUX_CONFIG_SHA256" \
    /usr/local/share/wt-tmux.conf | sha256sum --check --strict
printf '%s  %s\n' "$TMUX_CONFIG_SHA256" \
    /etc/skel/.byobu/.tmux.conf | sha256sum --check --strict
printf '%s  %s\n' "$BYOBU_COLOR_SHA256" \
    /etc/skel/.byobu/color | sha256sum --check --strict

phase "installing $WT_IMAGE_KIND application stack"
/bin/sh /var/tmp/wt-kind-image-build.sh

phase "removing shared build dependencies"
DEBIAN_FRONTEND=noninteractive apt-get autoremove --purge -y \
    bison build-essential curl libevent-dev libncurses-dev pkg-config
apt-get clean
! command -v cc
! command -v gcc
! command -v g++
! command -v make
! command -v curl

phase "validating shared terminal stack"
test "$(/usr/bin/tmux -V)" = "tmux $TMUX_VERSION"
test "$(dpkg-query -W -f='${Version}' byobu)" = "$BYOBU_VERSION"
printf '%s  %s\n' "$GHOSTTY_TERMINFO_SHA256" \
    /usr/share/terminfo/g/ghostty | sha256sum --check --strict
cmp /usr/share/terminfo/g/ghostty /usr/share/terminfo/x/xterm-ghostty
TERM=ghostty tput colors >/dev/null
TERM=xterm-ghostty tput colors >/dev/null

rm -f /var/tmp/wt-*.sh /var/tmp/wt-image-build.env \
    /var/tmp/wt-tmux.conf /var/tmp/wt-byobu-color /var/tmp/wt-host-shell
printf 'kind=%s\nstatus=ready\nrecipe_version=%s\n' \
    "$WT_IMAGE_KIND" "$WT_IMAGE_RECIPE_VERSION" > /var/lib/wt-image-result
chown root:root /var/lib/wt-image-result
chmod 0644 /var/lib/wt-image-result
phase "$WT_IMAGE_KIND image ready; requesting shutdown"
