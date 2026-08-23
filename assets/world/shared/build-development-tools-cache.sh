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

phase "installing cached operating-system and development packages"
/bin/sh /var/tmp/wt-install-packages.sh \
    openssh-server qemu-guest-agent tmux \
    bison build-essential cmake clang curl pkg-config wget jq yq docker.io \
    docker-compose-v2 shellcheck

phase "configuring cached operating-system services"
systemctl enable --now qemu-guest-agent.service
systemctl disable --now ssh.service ssh.socket
if ! getent group "$WT_GROUP" >/dev/null; then
    groupadd --gid "$WT_GID" "$WT_GROUP"
fi
if ! id "$WT_USER" >/dev/null 2>&1; then
    useradd --uid "$WT_UID" --gid "$WT_GROUP" --create-home \
        --home-dir "$WT_HOME" --shell /bin/bash "$WT_USER"
fi
test "$(id -u "$WT_USER")" = "$WT_UID"
test "$(id -g "$WT_USER")" = "$WT_GID"
test "$(getent passwd "$WT_USER" | cut -d: -f6)" = "$WT_HOME"
printf 'kernel.perf_event_paranoid = -1\n' > /etc/sysctl.d/99-wt-profiling.conf
sysctl --system
test "$(cat /proc/sys/kernel/perf_event_paranoid)" = -1

phase "installing cached development tools"
/bin/sh /var/tmp/wt-install-development-tools.sh
command -v cc gcc g++ make cmake clang pkg-config curl wget jq yq docker shellcheck
docker compose version >/dev/null
DEBIAN_FRONTEND=noninteractive apt-get purge -y \
    cloud-init cloud-initramfs-copymods cloud-initramfs-dyn-netconf
DEBIAN_FRONTEND=noninteractive apt-get autoremove --purge -y
apt-get clean
command -v cloud-init && exit 1

rm -f /var/tmp/wt-*.sh /var/tmp/wt-image-build.env
printf 'kind=%s\nstatus=ready\nwt_uid=%s\nwt_gid=%s\n' \
    "$WT_IMAGE_KIND" "$WT_UID" "$WT_GID" \
    > /var/lib/wt-image-result
chown root:root /var/lib/wt-image-result
chmod 0644 /var/lib/wt-image-result
phase "recipe complete; requesting VM shutdown"
