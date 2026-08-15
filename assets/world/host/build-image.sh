#!/bin/sh
set -eu

install -d -m 0755 /usr/local/libexec
install -m 0755 /var/tmp/wt-host-shell /usr/local/bin/wt-host-shell
cmp /var/tmp/wt-host-shell /usr/local/bin/wt-host-shell
install -m 0755 /var/tmp/wt-host-prepare /usr/local/libexec/wt-host-prepare
install -m 0755 /var/tmp/wt-host-inspect /usr/local/libexec/wt-host-inspect
install -m 0755 /var/tmp/wt-host-cloud-init /usr/local/libexec/wt-host-cloud-init
install -m 0755 /var/tmp/wt-host-setup /usr/local/bin/wt-host-setup
install -m 0644 /var/tmp/wt-host-defer-init /etc/cloud/cloud.cfg.d/99-wt-host-defer-init.cfg
install -m 0644 /var/tmp/wt-host-preserve-ssh \
    /usr/local/share/wt-host-cloud-init.yaml
install -d -m 0755 /etc/systemd/system/cloud-config.service.d \
    /etc/systemd/system/cloud-final.service.d
install -m 0644 /var/tmp/wt-host-cloud-config \
    /etc/systemd/system/cloud-config.service.d/wt-host.conf
install -m 0644 /var/tmp/wt-host-cloud-final \
    /etc/systemd/system/cloud-final.service.d/wt-host.conf
install -m 0644 /var/tmp/wt-host-setup-service \
    /etc/systemd/system/wt-host-setup.service
systemctl daemon-reload

! command -v docker
! command -v devcontainer
git --version
test ! -e /workspace
test ! -e /usr/local/bin/wt-app-shell
test ! -e /usr/local/bin/wt-agent-git-relay
systemd-analyze verify /etc/systemd/system/wt-host-setup.service

dpkg-query -W -f='${Package}\t${Version}\n' \
    openssh-server qemu-guest-agent byobu tmux |
    sort > /var/lib/wt-image-packages
