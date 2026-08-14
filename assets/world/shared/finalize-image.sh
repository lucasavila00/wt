#!/bin/sh
set -eu

. /var/tmp/wt-image-build.env

install -m 0755 /var/tmp/wt-tmux /usr/bin/tmux
test "$(/usr/bin/tmux -V)" = "tmux $TMUX_VERSION"
printf '%s  %s\n' "$GHOSTTY_TERMINFO_SHA256" \
    /usr/share/terminfo/g/ghostty | sha256sum --check --strict
cmp /usr/share/terminfo/g/ghostty /usr/share/terminfo/x/xterm-ghostty
TERM=ghostty tput colors >/dev/null
TERM=xterm-ghostty tput colors >/dev/null
printf '%s  %s\n' "$TMUX_CONFIG_SHA256" \
    /usr/local/share/wt-tmux.conf | sha256sum --check --strict
printf '%s  %s\n' "$TMUX_CONFIG_SHA256" \
    /etc/skel/.byobu/.tmux.conf | sha256sum --check --strict
printf '%s  %s\n' "$BYOBU_COLOR_SHA256" \
    /etc/skel/.byobu/color | sha256sum --check --strict

/usr/bin/tmux -V > /var/lib/wt-tmux-version
sha256sum /usr/bin/tmux | cut -d ' ' -f 1 > /var/lib/wt-tmux-sha256
cloud-init clean --logs --seed --configs network
rm -rf /var/lib/cloud/instance /var/lib/cloud/instances
rm -f /etc/netplan/50-cloud-init.yaml /var/lib/wt-image-result \
    /var/tmp/wt-*.sh /var/tmp/wt-image-build.env /var/tmp/wt-tmux \
    /var/tmp/wt-host-shell /var/tmp/wt-tmux.conf /var/tmp/wt-byobu-color \
    /var/lib/wt-tmux
truncate -s 0 /etc/machine-id
ln -sfn /etc/machine-id /var/lib/dbus/machine-id
