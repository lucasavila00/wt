# shellcheck shell=sh
wt_publish_shared_file() {
    wt_publish_check_validator=$1
    wt_publish_validator=$2
    wt_publish_source=$3
    wt_publish_share=$4
    wt_publish_temporary=$5
    wt_publish_destination=$6
    wt_publish_check_only=$7

    "$wt_publish_check_validator"
    if test -e "$wt_publish_share" || test -L "$wt_publish_share"; then
        wt_require_owned_directory "$wt_publish_share"
        wt_publish_share_mode=$(stat -c %a "$wt_publish_share")
        if test "$wt_publish_share_mode" != 700; then
            echo "directory mode drift at $wt_publish_share: expected mode=0700; actual mode=0$wt_publish_share_mode" >&2
            exit 1
        fi
    elif test "$wt_publish_check_only" = false; then
        install -d -m 0700 "$wt_publish_share"
    fi
    test "$wt_publish_check_only" = false || exit 0

    cleanup() {
        rm -f "$wt_publish_temporary"
    }
    trap cleanup EXIT HUP INT TERM

    while :; do
        "$wt_publish_validator"
        rm -f "$wt_publish_temporary"
        install -m 0600 "$wt_publish_source" "$wt_publish_temporary"
        mv -f "$wt_publish_temporary" "$wt_publish_destination"
        cmp -s "$wt_publish_source" "$wt_publish_destination" && exit 0
    done
}
