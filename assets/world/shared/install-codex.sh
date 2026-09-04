#!/bin/sh
set -eu

. /var/tmp/wt-image-build.env

runuser --user "$WT_USER" -- env HOME="$WT_HOME" CODEX_RELEASE="$CODEX_RELEASE" sh -c \
    'curl -fsSL https://chatgpt.com/codex/install.sh | CODEX_NON_INTERACTIVE=1 sh'
installed_version=$(runuser --user "$WT_USER" -- env HOME="$WT_HOME" \
    "$WT_HOME/.codex/packages/standalone/current/bin/codex" --version)
test "$installed_version" = "codex-cli $CODEX_RELEASE"
