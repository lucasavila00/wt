#!/bin/sh
set -eu

. /var/tmp/wt-image-build.env

/bin/sh /var/tmp/wt-install-packages.sh \
    ca-certificates git

install -m 0755 /var/tmp/wt-host-shell /usr/local/bin/wt-host-shell
install -m 0755 /var/tmp/wt-host-prepare /usr/local/libexec/wt-host-prepare

dpkg-query -W -f='${Package}\t${Version}\n' \
    ca-certificates git \
    openssh-server byobu tmux qemu-guest-agent |
    sort > /var/lib/wt-image-packages
