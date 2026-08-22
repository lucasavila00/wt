#!/bin/sh
set -eu

# The installer prepends wt-identity.sh when this asset runs from stdin. Source
# the sibling contract when invoking the checked-in asset directly.
if ! command -v wt_require_effective_identity >/dev/null 2>&1; then
    wt_asset_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
    # shellcheck source=wt-identity.sh
    . "$wt_asset_dir/wt-identity.sh"
fi

acl_entries() {
    getfacl -cp -- "$1" |
        sed -e '/^[[:space:]]*$/d' -e '/^[[:space:]]*#/d' -e 's/^[[:space:]]*//' |
        sort
}

ensure_qemu_acl() {
    path=$1
    actual=$(acl_entries "$path")
    expected=$(printf '%s\n' \
        'user::rwx' 'user:libvirt-qemu:--x' 'group::rwx' 'mask::rwx' 'other::---' |
        sort)
    test "$actual" = "$expected" && return

    legacy=$(printf '%s\n' 'user::rwx' 'group::rwx' 'other::---' | sort)
    if test "$actual" != "$legacy"; then
        echo "directory ACL drift at $path: expected only user:libvirt-qemu:--x in addition to mode 2770" >&2
        exit 1
    fi
    sudo setfacl -m u:libvirt-qemu:--x -- "$path"
}

active_group() {
    gid=$(getent group "$1" | cut -d: -f3)
    test -n "$gid" && id -G | tr ' ' '\n' | grep -Fx "$gid" >/dev/null
}

ensure_directory() {
    owner=$1
    group=$2
    mode=$3
    path=$4
    if test -e "$path" || test -L "$path"; then
        actual_uid=$(stat -c %u "$path")
        actual_gid=$(stat -c %g "$path")
        actual_mode=$(stat -c %a "$path")
        if ! test -d "$path" || test -L "$path" ||
            test "$actual_uid" != "$owner" ||
            test "$actual_gid" != "$group" ||
            test "$actual_mode" != "$mode"; then
            display_mode=$mode
            test "${#display_mode}" -ge 4 || display_mode=0$display_mode
            test "${#actual_mode}" -ge 4 || actual_mode=0$actual_mode
            echo "directory drift at $path: expected uid=$owner gid=$group mode=$display_mode; actual uid=$actual_uid gid=$actual_gid mode=$actual_mode" >&2
            exit 1
        fi
    else
        sudo install -d -o "$owner" -g "$group" -m "$mode" "$path"
    fi
}

case ${1-} in
    prepare)
        test "$#" -eq 5 || exit 2
        wt_require_effective_identity
        network=$2
        image_dir=$3
        binary_dir=$4
        worlds_dir=$5

        # shellcheck source=/dev/null
        . /etc/os-release
        if ! { test "${ID-}" = ubuntu && test "${VERSION_ID-}" = 24.04 &&
            test "$(dpkg --print-architecture)" = amd64; }; then
            echo 'Ubuntu 24.04 amd64 is required' >&2
            exit 1
        fi
        if ! { test -c /dev/kvm && test -r /dev/kvm && test -w /dev/kvm; }; then
            echo 'KVM is required: /dev/kvm must be a readable and writable character device' >&2
            exit 1
        fi
        for group in kvm libvirt; do
            active_group "$group" || {
                echo "group $group is not active; log out, log back in, and rerun" >&2
                exit 1
            }
        done
        kvm_gid=$(getent group kvm | cut -d: -f3)
        test "$(id -g libvirt-qemu)" = "$kvm_gid" || {
            echo 'libvirt-qemu must use kvm as its primary group' >&2
            exit 1
        }
        virsh -c qemu:///system domcapabilities --virttype kvm >/dev/null
        sudo -v

        network_info=$(virsh -c qemu:///system net-info "$network")
        printf '%s\n' "$network_info" | awk -F: '$1 == "Active" && $2 ~ /^[[:space:]]*yes[[:space:]]*$/ { found=1 } END { exit !found }' ||
            virsh -c qemu:///system net-start "$network"
        printf '%s\n' "$network_info" | awk -F: '$1 == "Autostart" && $2 ~ /^[[:space:]]*yes[[:space:]]*$/ { found=1 } END { exit !found }' ||
            virsh -c qemu:///system net-autostart "$network"

        ensure_directory 0 0 755 "$image_dir"
        ensure_directory 0 0 755 "$binary_dir"
        wt_require_owned_directory "$WT_IDENTITY_HOME"
        wt_require_owned_directory "$WT_IDENTITY_HOME/.codex"
        ensure_directory "$WT_IDENTITY_UID" "$WT_IDENTITY_GID" 700 /run/wt-image-build
        ensure_directory "$WT_IDENTITY_UID" "$kvm_gid" 2770 "$worlds_dir"
        ensure_directory "$WT_IDENTITY_UID" "$WT_IDENTITY_GID" 700 "$WT_IDENTITY_HOME/.codex/sessions"
        ensure_qemu_acl "$worlds_dir"
        ;;
    acl)
        test "$#" -eq 2 || exit 2
        wt_require_effective_identity
        ensure_qemu_acl "$2"
        ;;
    *)
        echo 'usage: install-host.sh {prepare NETWORK IMAGE_DIR BINARY_DIR WORLDS_DIR|acl PATH}' >&2
        exit 2
        ;;
esac
