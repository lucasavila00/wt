#!/bin/sh
set -eu

shutdown() {
    status=$?
    trap - EXIT
    sync
    systemctl poweroff || true
    exit "$status"
}
trap shutdown EXIT

. /var/tmp/wt-image-build.env

phase() {
    echo "WT_IMAGE_PHASE=$*" > /dev/ttyS0
}

phase "installing terminal build dependencies"
/bin/sh /var/tmp/wt-install-packages.sh libevent-dev libncurses-dev

phase "installing terminal tools"
/bin/sh /var/tmp/wt-install-terminal.sh

phase "installing Codex"
/bin/sh /var/tmp/wt-install-codex.sh

phase "installing Diffo"
/bin/sh /var/tmp/wt-install-diffo.sh
install -d -m 0755 /usr/local/share /usr/local/libexec
printf "WT_USER='%s'\nWT_GROUP='%s'\nWT_UID='%s'\nWT_GID='%s'\nWT_HOME='%s'\n" \
    "$WT_USER" "$WT_GROUP" "$WT_UID" "$WT_GID" "$WT_HOME" \
    > /usr/local/share/wt-host-contract
chmod 0644 /usr/local/share/wt-host-contract
install -m 0644 /var/tmp/wt-tmux.conf /usr/local/share/wt-tmux.conf
install -m 0755 /var/tmp/wt-host-access /usr/local/libexec/wt-host-access
install -m 0755 /var/tmp/wt-host-git-author \
    /usr/local/libexec/wt-host-git-author
install -m 0755 /var/tmp/wt-host-agent-tools /usr/local/libexec/wt-host-agent-tools
install -m 0755 /var/tmp/wt-host-mount-codex \
    /usr/local/libexec/wt-host-mount-codex
printf '%s  %s\n' "$TMUX_CONFIG_SHA256" \
    /usr/local/share/wt-tmux.conf | sha256sum --check --strict
printf '%s  %s\n' "$ACCESS_SHA256" \
    /usr/local/libexec/wt-host-access | sha256sum --check --strict
printf '%s  %s\n' "$GIT_AUTHOR_SHA256" \
    /usr/local/libexec/wt-host-git-author | sha256sum --check --strict
printf '%s  %s\n' "$AGENT_TOOLS_SHA256" \
    /usr/local/libexec/wt-host-agent-tools | sha256sum --check --strict
printf '%s  %s\n' "$MOUNT_CODEX_SHA256" \
    /usr/local/libexec/wt-host-mount-codex | sha256sum --check --strict

phase "installing guest tools"
/bin/sh /var/tmp/wt-host-image-build.sh

phase "validating cached development tools"
test -f /var/lib/wt-image-development-tools
runuser --user "$WT_USER" -- env HOME="$WT_HOME" \
    PATH="$WT_HOME/.local/bin:$WT_HOME/.cargo/bin:/usr/local/go/bin:/usr/local/bin:/usr/bin:/bin" \
    bash -o pipefail -c '
        . "$HOME/.nvm/nvm.sh"
        command -v cargo rustc go python node npm npx corepack uv docker shellcheck
        command -v nvm
        docker compose version >/dev/null
    '

phase "validating installed terminal tools"
test "$(/usr/bin/tmux -V)" = "tmux $TMUX_VERSION"
test "$(dpkg-query -W -f='${Version}' byobu)" = "$BYOBU_VERSION"
printf '%s  %s\n' "$GHOSTTY_TERMINFO_SHA256" \
    /usr/share/terminfo/g/ghostty | sha256sum --check --strict
cmp /usr/share/terminfo/g/ghostty /usr/share/terminfo/x/xterm-ghostty
TERM=ghostty tput colors >/dev/null
TERM=xterm-ghostty tput colors >/dev/null
DEBIAN_FRONTEND=noninteractive apt-get autoremove --purge -y \
    libevent-dev libncurses-dev
apt-get clean

rm -f /var/tmp/wt-*.sh /var/tmp/wt-image-build.env \
    /var/tmp/wt-tmux.conf /var/tmp/wt-byobu-color /var/tmp/wt-host-*
printf 'kind=%s\nstatus=ready\nwt_uid=%s\nwt_gid=%s\n' \
    "$WT_IMAGE_KIND" "$WT_UID" "$WT_GID" \
    > /var/lib/wt-image-result
chown root:root /var/lib/wt-image-result
chmod 0644 /var/lib/wt-image-result
phase "recipe complete; requesting VM shutdown"
