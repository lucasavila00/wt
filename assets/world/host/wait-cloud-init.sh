#!/bin/sh
set -u

cloud_pid=
tail_pid=

stop_children() {
    test -z "$tail_pid" || kill "$tail_pid" 2>/dev/null || true
    test -z "$cloud_pid" || kill "$cloud_pid" 2>/dev/null || true
}
trap stop_children EXIT HUP INT TERM

echo "Waiting for cloud-init..."
/usr/bin/cloud-init status --wait &
cloud_pid=$!
/usr/bin/tail --pid="$cloud_pid" -n +1 -F /var/log/cloud-init-output.log &
tail_pid=$!

status=0
wait "$cloud_pid" || status=$?
cloud_pid=
wait "$tail_pid" || true
tail_pid=
exit "$status"
