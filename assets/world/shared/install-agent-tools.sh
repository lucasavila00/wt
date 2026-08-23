#!/bin/sh
set -eu

. /usr/local/share/wt-host-contract

stage=/tmp/wt-host-agent-tools
vsock_port=$(cat "$stage-vsock-port")
case "$vsock_port" in
    ''|*[!0-9]*) echo "invalid agent tool vsock port" >&2; exit 1 ;;
esac

test -x /usr/local/bin/wt-agent-tool-gateway-relay
test -x /usr/local/bin/git-remote-wt-agent
test -x /usr/local/bin/wt-tools
install -d -m 0700 -o "$WT_USER" -g "$WT_GROUP" /var/lib/wt-agent-tool-gateway
install -m 0600 -o "$WT_USER" -g "$WT_GROUP" "$stage-grant" /var/lib/wt-agent-tool-gateway/grant
install -m 0600 -o "$WT_USER" -g "$WT_GROUP" \
    "$stage-providers" /var/lib/wt-agent-tool-gateway/providers
while IFS= read -r host; do
    test -n "$host" || continue
    runuser --user "$WT_USER" -- git config --global --replace-all \
        "url.wt-agent::git@$host:.insteadOf" "git@$host:"
    runuser --user "$WT_USER" -- git config --global --add \
        "url.wt-agent::git@$host:.insteadOf" "ssh://git@$host/"
    runuser --user "$WT_USER" -- git config --global --add \
        "url.wt-agent::git@$host:.insteadOf" "https://$host/"
done < /var/lib/wt-agent-tool-gateway/providers
cat > /etc/systemd/system/wt-agent-tool-gateway-relay.service <<EOF
[Unit]
Description=WT agent tool relay

[Service]
Type=simple
User=$WT_USER
ExecStart=/usr/local/bin/wt-agent-tool-gateway-relay --vsock-port $vsock_port
Restart=on-failure
RuntimeDirectory=wt-agent-tool-gateway
RuntimeDirectoryMode=0755
RuntimeDirectoryPreserve=restart
UMask=0077

[Install]
WantedBy=multi-user.target
EOF
rm -f "$stage-grant" "$stage-providers" "$stage-vsock-port"
systemctl daemon-reload
if ! systemctl enable --now wt-agent-tool-gateway-relay.service; then
    systemctl status --no-pager --full wt-agent-tool-gateway-relay.service >&2 || true
    journalctl --no-pager -u wt-agent-tool-gateway-relay.service -n 100 >&2 || true
    exit 1
fi
