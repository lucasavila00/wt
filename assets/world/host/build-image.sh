#!/bin/sh
set -eu

install -m 0755 /var/tmp/wt-host-shell /usr/local/bin/wt-host-shell
cmp /var/tmp/wt-host-shell /usr/local/bin/wt-host-shell

DEBIAN_FRONTEND=noninteractive apt-get purge -y \
    bison build-essential curl libevent-dev libncurses-dev pkg-config
apt-get clean
! command -v docker
! command -v devcontainer
! command -v git
! command -v curl
test ! -e /workspace
test ! -e /usr/local/bin/wt-app-shell
test ! -e /usr/local/bin/wt-agent-git-relay

dpkg-query -W -f='${Package}\t${Version}\n' \
    openssh-server qemu-guest-agent byobu tmux |
    sort > /var/lib/wt-image-packages
