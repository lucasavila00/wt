#!/bin/sh
set -eu

. /var/tmp/wt-image-build.env

/bin/sh /var/tmp/wt-install-packages.sh \
    ca-certificates git

install -m 0755 /var/tmp/wt-guest-shell /usr/local/bin/wt-guest-shell
install -m 0755 /var/tmp/wt-guest-prepare /usr/local/libexec/wt-guest-prepare
install -m 0755 /var/tmp/wtg /usr/local/bin/wtg
ln -s /usr/local/bin/wtg /usr/local/bin/git-remote-wt-agent
install -d -m 0755 /etc/codex
install -m 0644 /var/tmp/wt-codex-requirements.toml \
    /etc/codex/requirements.toml
install -d -m 0700 -o "$WT_USER" -g "$WT_GROUP" "$WT_HOME/.local/state/wt"
install -m 0600 -o "$WT_USER" -g "$WT_GROUP" \
    /var/tmp/wt-codex-config.toml "$WT_HOME/.codex/config.toml"

{
    dpkg-query -W -f='${Package}\t${Version}\n' \
        ca-certificates git openssh-server byobu tmux qemu-guest-agent
    dpkg-query -W -f='${Package}\t${Version}\n' \
        bison build-essential cmake clang curl wget jq yq pkg-config \
        docker.io docker-compose-v2 shellcheck
} | sort > /var/lib/wt-image-packages
