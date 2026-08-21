#!/bin/sh
set -eu

. /usr/local/share/wt-retained-contract

stage=/tmp/wt-retained-agent-git
vsock_port=$(cat "$stage-vsock-port")
case "$vsock_port" in
    ''|*[!0-9]*) echo "invalid agent Git vsock port" >&2; exit 1 ;;
esac

install -m 0755 "$stage-relay" /usr/local/bin/wt-agent-git-gateway-relay
install -m 0755 "$stage-remote" /usr/local/bin/git-remote-ag
install -m 0755 "$stage-cli" /usr/local/bin/ag-git
install -d -m 0700 -o "$WT_USER" -g "$WT_USER" /var/lib/wt-agent-git-gateway
install -m 0600 -o "$WT_USER" -g "$WT_USER" "$stage-grant" /var/lib/wt-agent-git-gateway/grant
install -m 0600 -o "$WT_USER" -g "$WT_USER" \
    "$stage-providers" /var/lib/wt-agent-git-gateway/providers
while IFS= read -r host; do
    test -n "$host" || continue
    runuser --user "$WT_USER" -- git config --global --replace-all \
        "url.ag::git@$host:.insteadOf" "git@$host:"
    runuser --user "$WT_USER" -- git config --global --add \
        "url.ag::git@$host:.insteadOf" "ssh://git@$host/"
    runuser --user "$WT_USER" -- git config --global --add \
        "url.ag::git@$host:.insteadOf" "https://$host/"
done < /var/lib/wt-agent-git-gateway/providers
cat > /etc/systemd/system/wt-agent-git-gateway-relay.service <<EOF
[Unit]
Description=WT agent Git relay

[Service]
Type=simple
User=$WT_USER
ExecStart=/usr/local/bin/wt-agent-git-gateway-relay --vsock-port $vsock_port
Restart=on-failure
RuntimeDirectory=wt-agent-git-gateway
RuntimeDirectoryMode=0755
RuntimeDirectoryPreserve=restart
UMask=0077

[Install]
WantedBy=multi-user.target
EOF
rm -f "$stage-grant" "$stage-relay" "$stage-remote" "$stage-cli" \
    "$stage-providers" "$stage-vsock-port"
systemctl daemon-reload
if ! systemctl enable --now wt-agent-git-gateway-relay.service; then
    systemctl status --no-pager --full wt-agent-git-gateway-relay.service >&2 || true
    journalctl --no-pager -u wt-agent-git-gateway-relay.service -n 100 >&2 || true
    exit 1
fi
