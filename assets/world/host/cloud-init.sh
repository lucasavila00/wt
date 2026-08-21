#!/bin/sh
set -eu

state=/var/lib/wt-host
log=/var/log/cloud-init-output.log
phase=init
umask 077

fail() {
    status=$?
    trap - EXIT
    if test "$status" -ne 0; then
        cloud-init status --long >> "$log" 2>&1 || true
        temporary=$state/error.wt-new
        printf 'cloud-init %s stage failed with exit status %s\n' \
            "$phase" "$status" > "$temporary"
        chmod 0644 "$temporary"
        mv -f "$temporary" "$state/error"
    fi
    exit "$status"
}
trap fail EXIT

test ! -e "$state/started"
install -m 0644 /dev/null "$state/started"

echo "WT host cloud-init: init" >> "$log"
cloud-init modules --mode=init --file /etc/cloud/cloud.cfg \
    --file "$state/user-data"

phase=config
echo "WT host cloud-init: config" >> "$log"
cloud-init modules --mode=config --file "$state/user-data"

phase=final
echo "WT host cloud-init: final" >> "$log"
cloud-init modules --mode=final --file "$state/user-data"

phase=complete
echo "WT host cloud-init complete." >> "$log"
install -m 0644 /dev/null "$state/complete"
