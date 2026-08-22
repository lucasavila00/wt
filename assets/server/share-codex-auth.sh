#!/bin/sh
set -eu

# The installer prepends wt-identity.sh when this asset is installed. Source
# the sibling contract when invoking the checked-in asset directly.
if ! command -v wt_require_effective_identity >/dev/null 2>&1; then
    wt_asset_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
    # shellcheck source=wt-identity.sh
    . "$wt_asset_dir/wt-identity.sh"
fi

codex_home=$WT_IDENTITY_HOME/.codex
auth=$codex_home/auth.json
share=$codex_home/.wt-auth
temporary=$codex_home/.wt-auth.wt-new.$$
shared_auth=$share/auth.json

wt_require_effective_identity

cleanup() {
    rm -f "$temporary"
}
trap cleanup EXIT HUP INT TERM

if test -e "$share" || test -L "$share"; then
    wt_require_owned_directory "$share"
    share_mode=$(stat -c %a "$share")
    if test "$share_mode" != 700; then
        echo "directory mode drift at $share: expected mode=0700; actual mode=0$share_mode" >&2
        exit 1
    fi
else
    install -d -m 0700 "$share"
fi
while :; do
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

    rm -f "$temporary"
    install -m 0600 "$auth" "$temporary"
    mv -f "$temporary" "$shared_auth"

    if cmp -s "$auth" "$shared_auth"; then
        exit 0
    fi
done
