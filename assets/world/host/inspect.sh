#!/bin/sh
set -eu

state=/var/lib/wt-host
if test -e "$state/complete"; then
    echo complete
elif test -e "$state/error"; then
    echo error
    cat "$state/error"
elif test -e "$state/started"; then
    if systemctl is-active --quiet wt-host-setup.service; then
        echo setup
    else
        echo error
        echo "host cloud-init was interrupted"
    fi
else
    echo setup
fi
