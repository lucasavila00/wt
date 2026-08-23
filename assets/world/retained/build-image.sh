#!/bin/sh
set -eu

. /var/tmp/wt-image-build.env

/bin/sh /var/tmp/wt-install-packages.sh \
    ca-certificates git

install -m 0755 /var/tmp/wt-host-shell /usr/local/bin/wt-host-shell
install -m 0755 /var/tmp/wt-host-prepare /usr/local/libexec/wt-host-prepare
install -m 0755 /var/tmp/wt-agent-tool-gateway-relay \
    /usr/local/bin/wt-agent-tool-gateway-relay
install -m 0755 /var/tmp/wt-git-remote-agent \
    /usr/local/bin/git-remote-wt-agent
install -m 0755 /var/tmp/wt-tools /usr/local/bin/wt-tools
install -m 0755 /var/tmp/wt-codex-integration \
    /usr/local/bin/wt-codex-integration
install -d -m 0755 /etc/codex
install -m 0644 /var/tmp/wt-codex-requirements.toml \
    /etc/codex/requirements.toml
install -d -m 0700 -o "$WT_USER" -g "$WT_GROUP" \
    "$WT_HOME/.local/state/wt"
temporary=$(mktemp)
cat > "$temporary" <<EOF
[Unit]
Description=Reconcile WT Codex session history
RequiresMountsFor=$WT_HOME/.codex/sessions

[Service]
Type=oneshot
User=$WT_USER
Group=$WT_GROUP
Environment=HOME=$WT_HOME
Environment=CODEX_HOME=$WT_HOME/.codex
ExecStart=/usr/local/bin/wt-codex-integration reconcile-worker
TimeoutStartSec=infinity
Restart=on-failure
RestartSec=30s
UMask=0077
EOF
install -m 0644 "$temporary" \
    /etc/systemd/system/wt-codex-reconciliation.service
cat > "$temporary" <<EOF
[Unit]
Description=Watch for WT Codex reconciliation requests

[Path]
PathChanged=$WT_HOME/.local/state/wt/codex-reconciliation-desired
Unit=wt-codex-reconciliation.service

[Install]
WantedBy=multi-user.target
EOF
install -m 0644 "$temporary" \
    /etc/systemd/system/wt-codex-reconciliation.path
ln -s ../wt-codex-reconciliation.path \
    /etc/systemd/system/multi-user.target.wants/wt-codex-reconciliation.path
rm -f "$temporary"
runuser --user "$WT_USER" -- env HOME="$WT_HOME" CODEX_HOME="$WT_HOME/.codex" \
    /usr/local/bin/wt-codex-integration install-config
test -x "$WT_HOME/.codex/packages/standalone/current/bin/codex"
ln -sfn /usr/local/bin/wt-codex-integration /usr/local/bin/codex
runuser --user "$WT_USER" -- ln -sfn /usr/local/bin/wt-codex-integration \
    "$WT_HOME/.local/bin/codex"

{
    dpkg-query -W -f='${Package}\t${Version}\n' \
        ca-certificates git openssh-server byobu tmux qemu-guest-agent
    dpkg-query -W -f='${Package}\t${Version}\n' \
        bison build-essential cmake clang curl wget jq yq pkg-config \
        docker.io docker-compose-v2 shellcheck
} | sort > /var/lib/wt-image-packages
