#!/bin/sh
set -eu

test "$#" -gt 0
log=/var/tmp/wt-image-apt.log
attempt=1
while ! (
    apt-get update &&
        DEBIAN_FRONTEND=noninteractive \
            apt-get install -y --no-install-recommends "$@"
) >"$log" 2>&1; do
    if test "$attempt" -ge 30; then
        echo "wt-image: package installation failed after $attempt attempts;" \
            "final apt output follows" >&2
        cat "$log" >&2
        exit 1
    fi
    echo "wt-image: package installation attempt $attempt/30 failed;" \
        "retrying in 2 seconds" >&2
    attempt=$((attempt + 1))
    sleep 2
done

echo "wt-image: packages installed on attempt $attempt/30" >&2
rm -f "$log"
