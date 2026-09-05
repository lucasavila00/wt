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
    /usr/local/libexec/wt-guest-access | sha256sum --check --strict
printf '%s  %s\n' "$GIT_AUTHOR_SHA256" \
    /usr/local/libexec/wt-guest-git-author | sha256sum --check --strict
printf '%s  %s\n' "$AGENT_TOOLS_SHA256" \
    /usr/local/libexec/wt-guest-agent-tools | sha256sum --check --strict
printf '%s  %s\n' "$MOUNT_CODEX_SHA256" \
    /usr/local/libexec/wt-guest-mount-codex | sha256sum --check --strict

/usr/bin/tmux -V > /var/lib/wt-tmux-version
sha256sum /usr/bin/tmux | cut -d ' ' -f 1 > /var/lib/wt-tmux-sha256
rm -rf /etc/cloud /var/lib/cloud /run/cloud-init
rm -f /etc/netplan/50-cloud-init.yaml /var/log/cloud-init.log \
    /var/log/cloud-init-output.log /var/lib/wt-image-result \
    /var/tmp/wt-*.sh /var/tmp/wt-image-build.env /var/tmp/wt-tmux \
    /var/tmp/wt-guest-shell /var/tmp/wt-tmux.conf /var/tmp/wt-byobu-color \
    /var/tmp/wt-guest-access /var/tmp/wt-guest-git-author \
    /var/tmp/wt-guest-agent-tools \
    /var/tmp/wt-guest-mount-codex \
    /var/tmp/wtg \
    /var/lib/wt-tmux
truncate -s 0 /etc/machine-id
ln -sfn /etc/machine-id /var/lib/dbus/machine-id
