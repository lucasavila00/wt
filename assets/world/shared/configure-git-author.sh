#!/bin/sh
set -eu

. /usr/local/share/wt-guest-contract

stage=/tmp/wt-guest-git-author
name=$(cat "$stage-name")
email=$(cat "$stage-email")
test -n "$name"
test -n "$email"

runuser --user "$WT_USER" -- git config --global --replace-all user.name "$name"
runuser --user "$WT_USER" -- git config --global --replace-all user.email "$email"
rm -f "$stage-name" "$stage-email"
