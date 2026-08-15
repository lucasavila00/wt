#!/bin/sh
set -eu

state=/var/lib/wt-host
log=/var/log/cloud-init-output.log
service=wt-host-setup.service

if test -e "$state/complete"; then
    exec /bin/bash -l
fi
if test -e "$state/error"; then
    cat "$state/error" >&2
    echo "Inspect the guest with the -vs SSH alias, then remove and recreate it." >&2
    exit 1
fi
if test -e "$state/started"; then
    case "$(systemctl show --property=ActiveState --value "$service")" in
        active|activating) ;;
        *)
            echo "Host cloud-init was interrupted and will not be run again." >&2
            echo "Inspect the guest with the -vs SSH alias, then remove and recreate it." >&2
            exit 1
            ;;
    esac
fi

sudo systemctl start "$service" &
setup_pid=$!
tail --pid="$setup_pid" -n +1 -F "$log" &
tail_pid=$!
trap 'kill "$tail_pid" 2>/dev/null || true' EXIT HUP INT TERM
status=0
wait "$setup_pid" || status=$?
wait "$tail_pid" 2>/dev/null || true
trap - EXIT HUP INT TERM

if test "$status" -ne 0; then
    test ! -e "$state/error" || cat "$state/error" >&2
    echo "Inspect the guest with the -vs SSH alias, then remove and recreate it." >&2
    exit "$status"
fi
test -e "$state/complete"
exec /bin/bash -l
