#!/bin/sh
set -eu

. /var/tmp/wt-image-build.env

/bin/sh /var/tmp/wt-install-packages.sh \
    ca-certificates docker.io docker-buildx docker-compose-v2 git nodejs npm

install -d -m 0755 /etc/docker
printf '{"seccomp-profile":"unconfined"}\n' > /etc/docker/daemon.json
systemctl enable --now docker.service
docker info
docker buildx version
docker compose version

npm install --global --fetch-retries=10 \
    "@devcontainers/cli@$DEVCONTAINER_CLI_VERSION"
test "$(devcontainer --version)" = "$DEVCONTAINER_CLI_VERSION"

install -m 0755 /var/tmp/wt-host-shell /usr/local/bin/wt-host-shell
install -m 0755 /var/tmp/wt-host-prepare /usr/local/libexec/wt-host-prepare
install -m 0755 /var/tmp/wt-host-inspect /usr/local/libexec/wt-host-inspect
install -m 0755 /var/tmp/wt-host-cloud-init /usr/local/libexec/wt-host-cloud-init
install -m 0755 /var/tmp/wt-host-setup /usr/local/bin/wt-host-setup
install -m 0644 /var/tmp/wt-host-defer-init /etc/cloud/cloud.cfg.d/99-wt-host-defer-init.cfg
install -d -m 0755 /etc/systemd/system/cloud-config.service.d \
    /etc/systemd/system/cloud-final.service.d
install -m 0644 /var/tmp/wt-host-cloud-config \
    /etc/systemd/system/cloud-config.service.d/wt-host.conf
install -m 0644 /var/tmp/wt-host-cloud-final \
    /etc/systemd/system/cloud-final.service.d/wt-host.conf
install -m 0644 /var/tmp/wt-host-setup-service \
    /etc/systemd/system/wt-host-setup.service
systemctl daemon-reload
systemd-analyze verify /etc/systemd/system/wt-host-setup.service

dpkg-query -W -f='${Package}\t${Version}\n' \
    ca-certificates docker.io docker-buildx docker-compose-v2 git \
    openssh-server nodejs npm byobu tmux qemu-guest-agent |
    sort > /var/lib/wt-image-packages
