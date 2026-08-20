#!/bin/sh
set -eu

if test "$#" -eq 0 || test $(( $# % 2 )) -ne 0; then
    echo 'usage: mount-folders.sh TAG TARGET [TAG TARGET ...]' >&2
    exit 2
fi

escape_fstab_path() {
    printf '%s' "$1" | sed -e 's/\\/\\134/g' -e 's/ /\\040/g' -e 's/	/\\011/g'
}

while test "$#" -gt 0; do
    tag=$1
    target=$2
    shift 2
    case "$tag" in
        wt-shared-*) tag_index=${tag#wt-shared-} ;;
        *) echo "invalid shared folder tag: $tag" >&2; exit 2 ;;
    esac
    case "$tag_index" in
        ''|*[!0-9]*) echo "invalid shared folder tag: $tag" >&2; exit 2 ;;
    esac
    case "$target" in
        ''|/*|.|..|./*|../*|*/.|*/..|*/./*|*/../*)
            echo "invalid shared folder target: $target" >&2
            exit 2
            ;;
    esac

    mountpoint=/home/wt
    remaining=$target
    while :; do
        component=${remaining%%/*}
        mountpoint=$mountpoint/$component
        if test -L "$mountpoint"; then
            echo "shared folder path must not contain a symbolic link: $mountpoint" >&2
            exit 1
        fi
        install -d -m 0700 -o wt -g wt -- "$mountpoint"
        case "$remaining" in
            */*) remaining=${remaining#*/} ;;
            *) break ;;
        esac
    done
    escaped_mountpoint=$(escape_fstab_path "$mountpoint")
    entry="$tag $escaped_mountpoint virtiofs rw,nosuid,nodev 0 0"
    if ! grep -Fqx -- "$entry" /etc/fstab; then
        if awk -v tag="$tag" -v target="$escaped_mountpoint" \
            '$1 == tag || $2 == target { found = 1 } END { exit !found }' /etc/fstab; then
            echo "conflicting shared folder entry in /etc/fstab for $tag or $mountpoint" >&2
            exit 1
        fi
        printf '%s\n' "$entry" >> /etc/fstab
    fi

    if ! findmnt --noheadings --mountpoint "$mountpoint" >/dev/null; then
        if ! mount -- "$mountpoint"; then
            echo "failed to mount shared folder $tag at $mountpoint" >&2
            exit 1
        fi
    fi
    mounted=$(findmnt --noheadings --output SOURCE,FSTYPE --mountpoint "$mountpoint" |
        awk 'NR == 1 { print $1 " " $2 }')
    if test "$mounted" != "$tag virtiofs"; then
        echo "expected virtiofs tag $tag at $mountpoint; found ${mounted:-nothing}" >&2
        exit 1
    fi
    chown wt:wt -- "$mountpoint"
    chmod 0700 -- "$mountpoint"
done
