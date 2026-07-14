#!/bin/sh
set -eu

if test "$#" -ne 7; then
    echo 'usage: install-guest-git.sh SOURCE CLONE CHECKOUT VALUE USER_NAME USER_EMAIL SSH_COMMAND' >&2
    exit 2
fi

source=$1
clone=$2
checkout=$3
checkout_value=$4
user_name=$5
user_email=$6
ssh_command=$7
stage=/tmp/wt-install-git
runtime=/run/wt-git
askpass=/tmp/wt-git-askpass

cleanup() {
    rm -rf "$runtime" "$askpass" \
        "$stage-identity" "$stage-known-hosts" "$stage-passphrase" "$stage-ssh"
}
trap cleanup EXIT HUP INT TERM

install -d -m 0700 "$runtime"
install -m 0600 "$stage-identity" "$runtime/identity"
install -m 0600 "$stage-known-hosts" "$runtime/known_hosts"
install -m 0600 "$stage-passphrase" "$runtime/passphrase"
printf '#!/bin/sh\ncat /run/wt-git/passphrase\n' > "$askpass"
chmod 0700 "$askpass"

case "$clone" in
    true)
        GIT_SSH_COMMAND='ssh -i /run/wt-git/identity -o IdentitiesOnly=yes -o UserKnownHostsFile=/run/wt-git/known_hosts -o StrictHostKeyChecking=yes' \
        SSH_ASKPASS="$askpass" SSH_ASKPASS_REQUIRE=force DISPLAY=wt:0 \
            /usr/bin/git -c safe.directory=/workspace clone -- "$source" /workspace
        ;;
    false) ;;
    *) echo "invalid clone mode: $clone" >&2; exit 2 ;;
esac

install -d -m 0755 /workspace/.git/wt
install -m 0444 "$stage-identity" /workspace/.git/wt/identity
install -m 0444 "$stage-known-hosts" /workspace/.git/wt/known_hosts
install -m 0555 "$stage-ssh" /workspace/.git/wt/ssh
/usr/bin/git -c safe.directory=/workspace -C /workspace config --local core.sshCommand "$ssh_command"

case "$checkout" in
    none) ;;
    branch) /usr/bin/git -c safe.directory=/workspace -C /workspace checkout -- "$checkout_value" ;;
    ref) /usr/bin/git -c safe.directory=/workspace -C /workspace checkout --detach -- "$checkout_value" ;;
    *) echo "invalid checkout mode: $checkout" >&2; exit 2 ;;
esac

/usr/bin/git -c safe.directory=/workspace -C /workspace config --local user.name "$user_name"
/usr/bin/git -c safe.directory=/workspace -C /workspace config --local user.email "$user_email"
