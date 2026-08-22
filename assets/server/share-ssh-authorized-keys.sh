#!/bin/sh
set -eu

# The installer prepends wt-identity.sh when this asset is installed. Source
# the sibling contract when invoking the checked-in asset directly.
if ! command -v wt_require_effective_identity >/dev/null 2>&1; then
    wt_asset_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
    # shellcheck source=wt-identity.sh
    . "$wt_asset_dir/wt-identity.sh"
fi
if ! command -v wt_publish_shared_file >/dev/null 2>&1; then
    # shellcheck source=publish-shared-file.sh
    . "$wt_asset_dir/publish-shared-file.sh"
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

wt_publish_shared_file require_keys require_keys "$source_keys" "$share" "$temporary" "$shared_keys" "$check_only"
