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

phase "refreshing retained-world tools from cached development image"
/bin/sh /var/tmp/wt-retained-image-build.sh

phase "validating cached development tools"
test -f /var/lib/wt-image-development-tools
runuser --user "$WT_USER" -- env HOME="$WT_HOME" \
    PATH="$WT_HOME/.local/bin:$WT_HOME/.cargo/bin:/usr/local/go/bin:/usr/local/bin:/usr/bin:/bin" \
    bash -o pipefail -c '
        . "$HOME/.nvm/nvm.sh"
        command -v cargo rustc go python nvm node npm uv docker
        docker compose version >/dev/null
    '

rm -f /var/tmp/wt-*.sh /var/tmp/wt-image-build.env \
    /var/tmp/wt-tmux.conf /var/tmp/wt-byobu-color /var/tmp/wt-host-*
printf 'kind=%s\nstatus=ready\nwt_uid=%s\nwt_gid=%s\n' \
    "$WT_IMAGE_KIND" "$WT_UID" "$WT_GID" \
    > /var/lib/wt-image-result
chown root:root /var/lib/wt-image-result
chmod 0644 /var/lib/wt-image-result
phase "recipe complete; requesting VM shutdown"
