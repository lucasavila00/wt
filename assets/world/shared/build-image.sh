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

phase "installing $WT_IMAGE_KIND application stack"
/bin/sh /var/tmp/wt-kind-image-build.sh

phase "validating shared terminal stack"
test "$(/usr/bin/tmux -V)" = "tmux $TMUX_VERSION"
test "$(dpkg-query -W -f='${Version}' byobu)" = "$BYOBU_VERSION"
printf '%s  %s\n' "$GHOSTTY_TERMINFO_SHA256" \
    /usr/share/terminfo/g/ghostty | sha256sum --check --strict
cmp /usr/share/terminfo/g/ghostty /usr/share/terminfo/x/xterm-ghostty
TERM=ghostty tput colors >/dev/null
TERM=xterm-ghostty tput colors >/dev/null

printf 'kind=%s\nstatus=ready\nrecipe_version=%s\n' \
    "$WT_IMAGE_KIND" "$WT_IMAGE_RECIPE_VERSION" > /var/lib/wt-image-result
chown root:root /var/lib/wt-image-result
chmod 0644 /var/lib/wt-image-result
phase "$WT_IMAGE_KIND image ready; requesting shutdown"
