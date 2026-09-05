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

# Keep agapi's provider binary and installer profile edits separate from the CLI.
agapi_codex_root="$WT_HOME/.local/share/agapi/codex"
agapi_staging=$(runuser --user "$WT_USER" -- mktemp -d)
trap 'rm -rf -- "$agapi_staging"' EXIT
runuser --user "$WT_USER" -- env HOME="$agapi_staging" \
    CODEX_HOME="$agapi_codex_root" CODEX_INSTALL_DIR="$agapi_codex_root/bin" \
    CODEX_RELEASE="$AGAPI_CODEX_RELEASE" CODEX_NON_INTERACTIVE=1 \
    PATH="$agapi_codex_root/bin:$PATH" sh /var/tmp/wt-codex-installer.sh
test "$(runuser --user "$WT_USER" -- "$agapi_codex_root/bin/codex" --version)" = \
    "codex-cli $AGAPI_CODEX_RELEASE"
