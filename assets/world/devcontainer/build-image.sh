#!/bin/sh
set -eu

/bin/sh /var/tmp/wt-install-packages.sh \
    ca-certificates docker.io docker-buildx docker-compose-v2 git nodejs npm

systemctl enable --now docker.service
docker info
docker buildx version
docker compose version

npm install --global "@devcontainers/cli@$DEVCONTAINER_CLI_VERSION"
test "$(devcontainer --version)" = "$DEVCONTAINER_CLI_VERSION"

dpkg-query -W -f='${Package}\t${Version}\n' \
    ca-certificates docker.io docker-buildx docker-compose-v2 git \
    openssh-server nodejs npm byobu tmux qemu-guest-agent |
    sort > /var/lib/wt-image-packages
