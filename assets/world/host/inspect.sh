#!/bin/sh
set -eu

state=/var/lib/wt-host
if test -e "$state/complete"; then
    echo complete
elif test -e "$state/error"; then
    echo error
    cat "$state/error"
elif test -e "$state/started"; then
    case "$(systemctl show --property=ActiveState --value wt-host-setup.service)" in
        active|activating) echo setup ;;
        *)
            echo error
            echo "host cloud-init was interrupted"
            ;;
    esac
else
    echo setup
fi
