#!/bin/sh
set -eu

. /var/tmp/wt-image-build.env

curl -fsSL https://chatgpt.com/codex/install.sh -o /var/tmp/wt-codex-installer.sh
runuser --user "$WT_USER" -- env HOME="$WT_HOME" CODEX_HOME="$WT_HOME/.codex" \
    CODEX_RELEASE="$CODEX_RELEASE" CODEX_NON_INTERACTIVE=1 \
    sh /var/tmp/wt-codex-installer.sh
test "$(runuser --user "$WT_USER" -- "$WT_HOME/.local/bin/codex" --version)" = \
    "codex-cli $CODEX_RELEASE"
ln -sfn "$WT_HOME/.local/bin/codex" /usr/local/bin/codex
