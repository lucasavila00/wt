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
test "$(id -u "$WT_USER")" = "$WT_UID"
test "$(id -g "$WT_USER")" = "$WT_GID"
test "$(getent passwd "$WT_USER" | cut -d: -f6)" = "$WT_HOME"
install -d -m 0755 -o "$WT_USER" -g "$WT_GROUP" "$WT_HOME/.byobu"
install -m 0644 -o "$WT_USER" -g "$WT_GROUP" \
    /var/tmp/wt-byobu-color "$WT_HOME/.byobu/color"
printf '%s  %s\n' "$BYOBU_COLOR_SHA256" \
    "$WT_HOME/.byobu/color" | sha256sum --check --strict
test "$(stat -c '%u:%g %a' "$WT_HOME/.byobu")" = "$WT_UID:$WT_GID 755"
test "$(stat -c '%u:%g %a' "$WT_HOME/.byobu/color")" = "$WT_UID:$WT_GID 644"
test -x "$WT_HOME/.local/bin/codex"
test "$(readlink /usr/local/bin/codex)" = \
    /usr/local/bin/wt-codex-integration
test "$(readlink "$WT_HOME/.local/bin/codex")" = \
    /usr/local/bin/wt-codex-integration
test -x "$WT_HOME/.codex/packages/standalone/current/bin/codex"
test "$(stat -c '%u:%g %a' "$WT_HOME/.local/state/wt")" = \
    "$WT_UID:$WT_GID 700"
test -f /etc/systemd/system/wt-codex-reconciliation.service
test "$(stat -c '%u:%g %a' \
    /etc/systemd/system/wt-codex-reconciliation.service)" = "0:0 644"
test -f /etc/systemd/system/wt-codex-reconciliation.path
test "$(readlink \
    /etc/systemd/system/multi-user.target.wants/wt-codex-reconciliation.path)" = \
    ../wt-codex-reconciliation.path
runuser --user "$WT_USER" -- env HOME="$WT_HOME" CODEX_HOME="$WT_HOME/.codex" \
    /usr/local/bin/codex --version > /dev/null
runuser --user "$WT_USER" -- env HOME="$WT_HOME" CODEX_HOME="$WT_HOME/.codex" \
    "$WT_HOME/.local/bin/codex" --version > /dev/null
test -f /var/lib/wt-image-development-tools
runuser --user "$WT_USER" -- env HOME="$WT_HOME" \
    PATH="$WT_HOME/.local/bin:$WT_HOME/.cargo/bin:/usr/local/go/bin:/usr/local/bin:/usr/bin:/bin" \
    bash -o pipefail -c '
        command -v cargo rustc go python node npm npx corepack uv docker shellcheck
        . "$HOME/.nvm/nvm.sh"
        command -v nvm
        docker compose version >/dev/null
    '
printf '%s  %s\n' "$ACCESS_SHA256" \
    /usr/local/libexec/wt-retained-access | sha256sum --check --strict
printf '%s  %s\n' "$GIT_AUTHOR_SHA256" \
    /usr/local/libexec/wt-retained-git-author | sha256sum --check --strict
printf '%s  %s\n' "$AGENT_TOOLS_SHA256" \
    /usr/local/libexec/wt-retained-agent-tools | sha256sum --check --strict
printf '%s  %s\n' "$MOUNT_CODEX_SHA256" \
    /usr/local/libexec/wt-retained-mount-codex | sha256sum --check --strict

/usr/bin/tmux -V > /var/lib/wt-tmux-version
sha256sum /usr/bin/tmux | cut -d ' ' -f 1 > /var/lib/wt-tmux-sha256
rm -rf /etc/cloud /var/lib/cloud /run/cloud-init
rm -f /etc/netplan/50-cloud-init.yaml /var/log/cloud-init.log \
    /var/log/cloud-init-output.log /var/lib/wt-image-result \
    /var/tmp/wt-*.sh /var/tmp/wt-image-build.env /var/tmp/wt-tmux \
    /var/tmp/wt-host-shell /var/tmp/wt-tmux.conf /var/tmp/wt-byobu-color \
    /var/tmp/wt-retained-access /var/tmp/wt-retained-git-author \
    /var/tmp/wt-retained-agent-tools \
    /var/tmp/wt-retained-mount-codex \
    /var/tmp/wt-agent-tool-gateway-relay \
    /var/tmp/wt-git-remote-agent \
    /var/tmp/wt-tools \
    /var/tmp/wt-codex-integration \
    /var/lib/wt-tmux
truncate -s 0 /etc/machine-id
ln -sfn /etc/machine-id /var/lib/dbus/machine-id
