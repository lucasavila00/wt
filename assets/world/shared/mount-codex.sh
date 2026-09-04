#!/bin/sh
set -eu

sessions=/home/wt/.codex/sessions
auth_mount=/run/wt-codex-integration-auth
auth=/home/wt/.codex/auth.json
ssh_keys_mount=/run/wt-ssh-authorized-keys

escape_fstab_path() {
    printf '%s' "$1" | sed -e 's/\\/\\134/g' -e 's/ /\\040/g' -e 's/	/\\011/g'
}

install -d -m 0700 -o wt -g wt /home/wt/.codex
if test -L "$sessions"; then
    echo "Codex sessions path must not be a symbolic link: $sessions" >&2
    exit 1
fi
if ! test -e "$sessions"; then
    install -d -m 0700 -o wt -g wt "$sessions"
elif ! test -d "$sessions"; then
    echo "Codex sessions path is not a directory: $sessions" >&2
    exit 1
fi
if ! findmnt --noheadings --mountpoint "$auth_mount" >/dev/null; then
    install -d -m 0700 -o root -g root "$auth_mount"
fi
if ! findmnt --noheadings --mountpoint "$ssh_keys_mount" >/dev/null; then
    install -d -m 0700 -o root -g root "$ssh_keys_mount"
fi

sessions_entry="wt-codex-integration-sessions $(escape_fstab_path "$sessions") virtiofs rw,nosuid,nodev 0 0"
auth_entry="wt-codex-integration-auth $(escape_fstab_path "$auth_mount") virtiofs ro,nosuid,nodev,noexec 0 0"
ssh_keys_entry="wt-ssh-authorized-keys $(escape_fstab_path "$ssh_keys_mount") virtiofs ro,nosuid,nodev,noexec 0 0"
for entry in "$sessions_entry" "$auth_entry" "$ssh_keys_entry"; do
    tag=${entry%% *}
    rest=${entry#* }
    mountpoint=${rest%% *}
    if ! grep -Fqx -- "$entry" /etc/fstab; then
        if awk -v tag="$tag" -v target="$mountpoint" \
            '$1 == tag || $2 == target { found = 1 } END { exit !found }' /etc/fstab; then
            echo "conflicting Codex mount in /etc/fstab for $tag or $mountpoint" >&2
            exit 1
        fi
        printf '%s\n' "$entry" >> /etc/fstab
    fi
    if ! findmnt --noheadings --mountpoint "$mountpoint" >/dev/null; then
        mount -- "$mountpoint"
    fi
    mounted=$(findmnt --noheadings --output SOURCE,FSTYPE --mountpoint "$mountpoint" |
        awk 'NR == 1 { print $1 " " $2 }')
    if test "$mounted" != "$tag virtiofs"; then
        echo "expected virtiofs tag $tag at $mountpoint; found ${mounted:-nothing}" >&2
        exit 1
    fi
done

test -f "$auth_mount/auth.json" || {
    echo "Codex authentication share does not contain auth.json" >&2
    exit 1
}
test -f "$ssh_keys_mount/authorized_keys" || {
    echo "SSH access share does not contain authorized_keys" >&2
    exit 1
}
if test -e "$auth" && ! test -L "$auth"; then
    echo "Codex authentication target is not the WT-managed link: $auth" >&2
    exit 1
fi
ln -sfn "$auth_mount/auth.json" "$auth"
test -r "$auth"

# Start only after persistent history and shared authentication are mounted.
install -d -m 0700 -o wt -g wt /home/wt/.local/state/wt/codex
cat > /etc/systemd/system/wt-codex-app-server.service <<'EOF'
[Unit]
Description=WT Codex App Server
RequiresMountsFor=/home/wt/.codex/sessions /run/wt-codex-integration-auth
After=network-online.target
StartLimitIntervalSec=0

[Service]
User=wt
Environment=HOME=/home/wt
Environment=CODEX_HOME=/home/wt/.codex
WorkingDirectory=/home/wt
ExecStart=/home/wt/.codex/packages/standalone/current/bin/codex app-server --listen unix:///home/wt/.local/state/wt/codex/app-server.sock
Restart=always
RestartSec=2
UMask=0077

[Install]
WantedBy=multi-user.target
EOF

cat > /etc/systemd/system/wt-codex-completions.service <<'EOF'
[Unit]
Description=WT Codex completion recovery
RequiresMountsFor=/home/wt/.codex/sessions /run/wt-codex-integration-auth
Wants=wt-codex-app-server.service wt-agent-tool-gateway-relay.service
After=wt-codex-app-server.service wt-agent-tool-gateway-relay.service
StartLimitIntervalSec=0

[Service]
User=wt
Environment=HOME=/home/wt
Environment=CODEX_HOME=/home/wt/.codex
WorkingDirectory=/home/wt
ExecStart=/usr/local/bin/wtg codex watch-turns
Restart=always
RestartSec=2
UMask=0077

[Install]
WantedBy=multi-user.target
EOF
systemctl daemon-reload
systemctl enable --now wt-codex-app-server.service wt-codex-completions.service
