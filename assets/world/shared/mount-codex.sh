#!/bin/sh
set -eu

sessions=/home/wt/.codex/sessions
auth_mount=/run/wt-codex-auth
auth=/home/wt/.codex/auth.json

escape_fstab_path() {
    printf '%s' "$1" | sed -e 's/\\/\\134/g' -e 's/ /\\040/g' -e 's/	/\\011/g'
}

install -d -m 0700 -o wt -g wt /home/wt/.codex
if test -L "$sessions"; then
    echo "Codex sessions path must not be a symbolic link: $sessions" >&2
    exit 1
fi
if ! test -e "$sessions"; then
    install -d -m 0700 -o wt -g wt "$sessions"
elif ! test -d "$sessions"; then
    echo "Codex sessions path is not a directory: $sessions" >&2
    exit 1
fi
install -d -m 0700 -o root -g root "$auth_mount"

sessions_entry="wt-codex-sessions $(escape_fstab_path "$sessions") virtiofs rw,nosuid,nodev 0 0"
auth_entry="wt-codex-auth $(escape_fstab_path "$auth_mount") virtiofs ro,nosuid,nodev,noexec 0 0"
for entry in "$sessions_entry" "$auth_entry"; do
    tag=${entry%% *}
    rest=${entry#* }
    mountpoint=${rest%% *}
    if ! grep -Fqx -- "$entry" /etc/fstab; then
        if awk -v tag="$tag" -v target="$mountpoint" \
            '$1 == tag || $2 == target { found = 1 } END { exit !found }' /etc/fstab; then
            echo "conflicting Codex mount in /etc/fstab for $tag or $mountpoint" >&2
            exit 1
        fi
        printf '%s\n' "$entry" >> /etc/fstab
    fi
    if ! findmnt --noheadings --mountpoint "$mountpoint" >/dev/null; then
        mount -- "$mountpoint"
    fi
    mounted=$(findmnt --noheadings --output SOURCE,FSTYPE --mountpoint "$mountpoint" |
        awk 'NR == 1 { print $1 " " $2 }')
    if test "$mounted" != "$tag virtiofs"; then
        echo "expected virtiofs tag $tag at $mountpoint; found ${mounted:-nothing}" >&2
        exit 1
    fi
done

test -f "$auth_mount/auth.json" || {
    echo "Codex authentication share does not contain auth.json" >&2
    exit 1
}
if test -e "$auth" && ! test -L "$auth"; then
    echo "Codex authentication target is not the WT-managed link: $auth" >&2
    exit 1
fi
ln -sfn "$auth_mount/auth.json" "$auth"
test -r "$auth"
