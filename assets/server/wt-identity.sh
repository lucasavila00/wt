# shellcheck shell=sh
# Canonical WT host/guest filesystem identity. Keep the Rust identity contract
# in sync; repository tests compare every field.
WT_IDENTITY_USER=wt
WT_IDENTITY_GROUP=wt
WT_IDENTITY_UID=1001
WT_IDENTITY_GID=1001
WT_IDENTITY_HOME=/home/wt
readonly WT_IDENTITY_USER WT_IDENTITY_GROUP WT_IDENTITY_UID WT_IDENTITY_GID WT_IDENTITY_HOME

wt_require_effective_identity() {
    wt_actual_user=$(id -un)
    wt_actual_group=$(id -gn)
    wt_actual_uid=$(id -u)
    wt_actual_gid=$(id -g)
    wt_passwd_entry=$(getent passwd "$wt_actual_uid" || true)
    if test -n "$wt_passwd_entry"; then
        wt_actual_primary_gid=$(printf '%s\n' "$wt_passwd_entry" | cut -d: -f4)
        wt_actual_home=$(printf '%s\n' "$wt_passwd_entry" | cut -d: -f6)
    else
        wt_actual_primary_gid='<unknown>'
        wt_actual_home='<unknown>'
    fi
    if test "$wt_actual_user" != "$WT_IDENTITY_USER" ||
        test "$wt_actual_group" != "$WT_IDENTITY_GROUP" ||
        test "$wt_actual_uid" != "$WT_IDENTITY_UID" ||
        test "$wt_actual_gid" != "$WT_IDENTITY_GID" ||
        test "$wt_actual_primary_gid" != "$WT_IDENTITY_GID" ||
        test "$wt_actual_home" != "$WT_IDENTITY_HOME"; then
        echo "WT identity mismatch: expected user/group=$WT_IDENTITY_USER:$WT_IDENTITY_GROUP uid/gid=$WT_IDENTITY_UID:$WT_IDENTITY_GID home=$WT_IDENTITY_HOME; actual user/group=$wt_actual_user:$wt_actual_group effective uid/gid=$wt_actual_uid:$wt_actual_gid account primary gid=$wt_actual_primary_gid home=$wt_actual_home" >&2
        return 1
    fi
}

wt_require_owned_directory() {
    wt_identity_path=$1
    if test -L "$wt_identity_path"; then
        wt_actual_type=symlink
    elif test -d "$wt_identity_path"; then
        wt_actual_type=directory
    elif test -e "$wt_identity_path"; then
        wt_actual_type=other
    else
        wt_actual_type=missing
    fi
    if test "$wt_actual_type" = missing; then
        wt_actual_uid='<unknown>'
        wt_actual_gid='<unknown>'
    else
        wt_actual_uid=$(stat -c %u "$wt_identity_path")
        wt_actual_gid=$(stat -c %g "$wt_identity_path")
    fi
    if test "$wt_actual_type" != directory ||
        test "$wt_actual_uid" != "$WT_IDENTITY_UID" ||
        test "$wt_actual_gid" != "$WT_IDENTITY_GID"; then
        echo "WT directory identity mismatch at $wt_identity_path: expected type=directory uid/gid=$WT_IDENTITY_UID:$WT_IDENTITY_GID; actual type=$wt_actual_type uid/gid=$wt_actual_uid:$wt_actual_gid" >&2
        return 1
    fi
}
