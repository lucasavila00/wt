#!/bin/sh
set -eu

log=/var/tmp/wt-host-image-apt.log
attempt=1
while ! (
    apt-get update &&
        DEBIAN_FRONTEND=noninteractive \
            apt-get install -y --no-install-recommends \
                openssh-server qemu-guest-agent
) >"$log" 2>&1; do
    if test "$attempt" -ge 30; then
        echo "wt-host-image: package installation failed after $attempt attempts;" \
            "final apt output follows" >&2
        cat "$log" >&2
        exit 1
    fi
    echo "wt-host-image: package installation attempt $attempt/30 failed;" \
        "retrying in 2 seconds" >&2
    attempt=$((attempt + 1))
    sleep 2
done

echo "wt-host-image: packages installed on attempt $attempt/30" >&2
rm -f "$log" /etc/resolv.conf
ln -s ../run/systemd/resolve/stub-resolv.conf /etc/resolv.conf
