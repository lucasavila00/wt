#!/bin/sh
set -eu

. /var/tmp/wt-image-build.env
: "${BYOBU_VERSION:?}"
: "${BYOBU_SHA256:?}"
: "${TMUX_VERSION:?}"
: "${TMUX_SHA256:?}"
: "${NCURSES_TERM_DEB:?}"
: "${NCURSES_TERM_SHA256:?}"
: "${GHOSTTY_TERMINFO_SHA256:?}"

printf '%s  %s\n' "$BYOBU_SHA256" /var/tmp/wt-byobu.deb |
    sha256sum --check --strict
/bin/sh /var/tmp/wt-install-packages.sh /var/tmp/wt-byobu.deb
test "$(dpkg-query -W -f='${Version}' byobu)" = "$BYOBU_VERSION"

curl -fL --retry 10 --retry-all-errors --retry-delay 2 \
    --output /tmp/tmux.tar.gz \
    "https://github.com/tmux/tmux/releases/download/$TMUX_VERSION/tmux-$TMUX_VERSION.tar.gz"
printf '%s  %s\n' "$TMUX_SHA256" /tmp/tmux.tar.gz |
    sha256sum --check --strict
tar -xzf /tmp/tmux.tar.gz -C /tmp
cd "/tmp/tmux-$TMUX_VERSION"
./configure --prefix=/usr
make -j2
make install
install -m 0755 /usr/bin/tmux /var/lib/wt-tmux
cd /

curl -fL --retry 10 --retry-all-errors --retry-delay 2 \
    --output /tmp/ncurses-term.deb \
    "https://archive.ubuntu.com/ubuntu/pool/main/n/ncurses/$NCURSES_TERM_DEB"
printf '%s  %s\n' "$NCURSES_TERM_SHA256" /tmp/ncurses-term.deb |
    sha256sum --check --strict
install -d -m 0755 /usr/share/terminfo/g /usr/share/terminfo/x
dpkg-deb --fsys-tarfile /tmp/ncurses-term.deb |
    tar -xO ./usr/share/terminfo/g/ghostty > /usr/share/terminfo/g/ghostty
cp /usr/share/terminfo/g/ghostty /usr/share/terminfo/x/xterm-ghostty

test "$(/usr/bin/tmux -V)" = "tmux $TMUX_VERSION"
printf '%s  %s\n' "$GHOSTTY_TERMINFO_SHA256" \
    /usr/share/terminfo/g/ghostty | sha256sum --check --strict
cmp /usr/share/terminfo/g/ghostty /usr/share/terminfo/x/xterm-ghostty
TERM=ghostty tput colors >/dev/null
TERM=xterm-ghostty tput colors >/dev/null

rm -rf /tmp/tmux.tar.gz "/tmp/tmux-$TMUX_VERSION" /tmp/ncurses-term.deb
rm -f /var/tmp/wt-byobu.deb
