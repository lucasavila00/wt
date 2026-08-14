#!/bin/sh
set -eu

install -d -m 0755 /usr/local/share /etc/skel/.byobu
install -m 0644 /var/tmp/wt-tmux.conf /usr/local/share/wt-tmux.conf
install -m 0644 /var/tmp/wt-tmux.conf /etc/skel/.byobu/.tmux.conf
install -m 0644 /var/tmp/byobu-color /etc/skel/.byobu/color
install -m 0755 /var/tmp/wt-host-shell /usr/local/bin/wt-host-shell
! command -v docker
! command -v devcontainer
! command -v git
test ! -e /workspace
test ! -e /usr/local/bin/wt-app-shell
test ! -e /usr/local/bin/wt-agent-git-relay

dpkg-query -W -f='${Package}\t${Version}\n' \
    openssh-server qemu-guest-agent byobu tmux |
    sort > /var/lib/wt-image-packages
