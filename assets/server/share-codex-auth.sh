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

case $# in
    0) check_only=false; auth=$WT_IDENTITY_HOME/.codex/auth.json; share=$WT_IDENTITY_HOME/.codex/.wt-auth ;;
    1)
        test "$1" = --check || { echo 'usage: share-codex-auth.sh [--check] [AUTH SHARE]' >&2; exit 2; }
        check_only=true
        auth=$WT_IDENTITY_HOME/.codex/auth.json
        share=$WT_IDENTITY_HOME/.codex/.wt-auth
        ;;
    2) check_only=false; auth=$1; share=$2 ;;
    3)
        test "$1" = --check || { echo 'usage: share-codex-auth.sh [--check] [AUTH SHARE]' >&2; exit 2; }
        check_only=true
        auth=$2
        share=$3
        ;;
    *) echo 'usage: share-codex-auth.sh [--check] [AUTH SHARE]' >&2; exit 2 ;;
esac

temporary=$share.wt-new.$$
shared_auth=$share/auth.json

wt_require_effective_identity

validate_auth() {
    if test -L "$auth" || ! test -f "$auth"; then
        echo "Codex authentication must be a regular, non-symlink file: $auth" >&2
        exit 1
    fi
    auth_uid=$(stat -c %u "$auth")
    auth_gid=$(stat -c %g "$auth")
    if test "$auth_uid" != "$WT_IDENTITY_UID" || test "$auth_gid" != "$WT_IDENTITY_GID"; then
        echo "Codex authentication ownership mismatch at $auth: expected uid=$WT_IDENTITY_UID gid=$WT_IDENTITY_GID; actual uid=$auth_uid gid=$auth_gid" >&2
        exit 1
    fi
}

prepare_auth() {
    validate_auth
    chmod 0600 "$auth"
}

wt_publish_shared_file validate_auth prepare_auth "$auth" "$share" "$temporary" "$shared_auth" "$check_only"
