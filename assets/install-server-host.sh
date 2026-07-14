#!/bin/sh
set -eu

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
    if test -e "$path"; then
        if ! test -d "$path" ||
            test "$(stat -Lc %u "$path")" != "$owner" ||
            test "$(stat -Lc %g "$path")" != "$group" ||
            test "$(stat -Lc %a "$path")" != "$mode"; then
            display_mode=$mode
            test "${#display_mode}" -ge 4 || display_mode=0$display_mode
            echo "directory drift at $path: expected uid=$owner, gid=$group, mode=$display_mode" >&2
            exit 1
        fi
    else
        sudo install -d -o "$owner" -g "$group" -m "$mode" "$path"
    fi
}

case ${1-} in
    prepare)
        test "$#" -eq 6 || exit 2
        network=$2
        image_dir=$3
        binary_dir=$4
        worlds_dir=$5
        registry_dir=$6

        # shellcheck source=/dev/null
        . /etc/os-release
        test "${ID-}" = ubuntu && test "${VERSION_ID-}" = 24.04 &&
            test "$(dpkg --print-architecture)" = amd64 || {
                echo 'Ubuntu 24.04 amd64 is required' >&2
                exit 1
            }
        test -c /dev/kvm && test -r /dev/kvm && test -w /dev/kvm || {
            echo 'KVM is required: /dev/kvm must be a readable and writable character device' >&2
            exit 1
        }
        for group in kvm libvirt docker; do
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
        ensure_directory "$(id -u)" "$kvm_gid" 2770 "$worlds_dir"
        ensure_directory 0 0 755 "$registry_dir"
        ensure_qemu_acl "$worlds_dir"
        ;;
    acl)
        test "$#" -eq 2 || exit 2
        ensure_qemu_acl "$2"
        ;;
    *)
        echo 'usage: install-server-host.sh {prepare NETWORK IMAGE_DIR BINARY_DIR WORLDS_DIR REGISTRY_DIR|acl PATH}' >&2
        exit 2
        ;;
esac
