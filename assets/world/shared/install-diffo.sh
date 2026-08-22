#!/bin/sh
set -eu

curl --proto '=https' --tlsv1.2 -LsSf \
    https://raw.githubusercontent.com/lucasavila00/diffo/main/install.sh | sh
test -x /usr/local/bin/diffo
