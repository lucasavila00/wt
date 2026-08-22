#!/bin/sh
set -eu

# The installer prepends wt-identity.sh when this asset is installed. Source
# the sibling contract when invoking the checked-in asset directly.
if ! command -v wt_require_effective_identity >/dev/null 2>&1; then
    wt_asset_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
    # shellcheck source=wt-identity.sh
    . "$wt_asset_dir/wt-identity.sh"
fi

source_keys=$WT_IDENTITY_HOME/.ssh/authorized_keys
share=$WT_IDENTITY_HOME/.ssh/.wt-authorized-keys
temporary=$WT_IDENTITY_HOME/.ssh/.wt-authorized-keys.wt-new.$$
shared_keys=$share/authorized_keys

case ${1-} in
    '') check_only=false ;;
    --check) check_only=true ;;
    *) echo 'usage: share-ssh-authorized-keys.sh [--check]' >&2; exit 2 ;;
esac

wt_require_effective_identity

cleanup() {
    rm -f "$temporary"
}

require_keys() {
    if test -L "$source_keys" || ! test -s "$source_keys"; then
        echo "SSH authorized keys must be a nonempty regular, non-symlink file: $source_keys" >&2
        exit 1
    fi
    keys_uid=$(stat -c %u "$source_keys")
    keys_gid=$(stat -c %g "$source_keys")
    if test "$keys_uid" != "$WT_IDENTITY_UID" || test "$keys_gid" != "$WT_IDENTITY_GID"; then
        echo "SSH authorized keys ownership mismatch at $source_keys: expected uid=$WT_IDENTITY_UID gid=$WT_IDENTITY_GID; actual uid=$keys_uid gid=$keys_gid" >&2
        exit 1
    fi
    ssh-keygen -l -f "$source_keys" >/dev/null
}

require_keys
if test -e "$share" || test -L "$share"; then
    wt_require_owned_directory "$share"
    share_mode=$(stat -c %a "$share")
    if test "$share_mode" != 700; then
        echo "directory mode drift at $share: expected mode=0700; actual mode=0$share_mode" >&2
        exit 1
    fi
elif test "$check_only" = false; then
    install -d -m 0700 "$share"
fi
test "$check_only" = false || exit 0
trap cleanup EXIT HUP INT TERM

while :; do
    require_keys
    rm -f "$temporary"
    install -m 0600 "$source_keys" "$temporary"
    mv -f "$temporary" "$shared_keys"
    cmp -s "$source_keys" "$shared_keys" && exit 0
done
