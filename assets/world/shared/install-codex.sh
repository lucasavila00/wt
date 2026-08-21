#!/bin/sh
set -eu

. /var/tmp/wt-image-build.env

runuser --user "$WT_USER" -- env HOME="$WT_HOME" sh -c \
    'curl -fsSL https://chatgpt.com/codex/install.sh | CODEX_NON_INTERACTIVE=1 sh'
ln -sf "$WT_HOME/.local/bin/codex" /usr/local/bin/codex
runuser --user "$WT_USER" -- env HOME="$WT_HOME" /usr/local/bin/codex --version
