#!/bin/sh
set -eu
directory=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
runtime=$(mktemp -d "${TMPDIR:-/tmp}/wt-git.XXXXXX")
trap 'rm -rf "$runtime"' EXIT HUP INT TERM
install -m 0600 "$directory/identity" "$runtime/identity"
/usr/bin/ssh \
  -i "$runtime/identity" \
  -o IdentitiesOnly=yes \
  -o UserKnownHostsFile="$directory/known_hosts" \
  -o StrictHostKeyChecking=yes \
  "$@"
