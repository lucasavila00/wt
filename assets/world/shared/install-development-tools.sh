#!/bin/sh
set -eu

. /var/tmp/wt-image-build.env

phase() {
    echo "WT_IMAGE_PHASE=$*" > /dev/ttyS0
}

phase "installing Go"
go_release=/var/tmp/wt-go-release.json
curl -fsSL https://go.dev/dl/?mode=json > "$go_release"
go_archive=$(jq -er '[.[] | select(.stable) | .files[] | select(.os == "linux" and .arch == "amd64" and .kind == "archive")][0].filename' "$go_release")
go_sha256=$(jq -er '[.[] | select(.stable) | .files[] | select(.os == "linux" and .arch == "amd64" and .kind == "archive")][0].sha256' "$go_release")
curl -fsSL --output "/var/tmp/$go_archive" "https://go.dev/dl/$go_archive"
printf '%s  %s\n' "$go_sha256" "/var/tmp/$go_archive" | sha256sum --check --strict
rm -rf /usr/local/go
tar -C /usr/local -xzf "/var/tmp/$go_archive"

phase "installing Rust and Cargo"
runuser --user "$WT_USER" -- env HOME="$WT_HOME" \
    PATH="$WT_HOME/.local/bin:$WT_HOME/.cargo/bin:/usr/local/go/bin:/usr/local/bin:/usr/bin:/bin" \
    bash -o pipefail -c '
        set -eu
        curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs |
            sh -s -- -y --default-toolchain stable
        "$HOME/.cargo/bin/rustup" component add rustfmt clippy
    '

phase "installing Node.js and nvm"
runuser --user "$WT_USER" -- env HOME="$WT_HOME" NODE_VERSION="$NODE_VERSION" \
    PATH="$WT_HOME/.local/bin:$WT_HOME/.cargo/bin:/usr/local/go/bin:/usr/local/bin:/usr/bin:/bin" \
    bash -o pipefail -c '
        set -eu
        nvm_tag=$(git ls-remote --refs --tags https://github.com/nvm-sh/nvm.git "v[0-9]*" |
            cut -f2 | sed "s#refs/tags/##" | sort -V | tail -n1)
        test -n "$nvm_tag"
        git clone --depth 1 --branch "$nvm_tag" https://github.com/nvm-sh/nvm.git "$HOME/.nvm"
        . "$HOME/.nvm/nvm.sh"
        nvm install "$NODE_VERSION"
        nvm alias default "$NODE_VERSION"
    '

phase "installing Python and uv"
runuser --user "$WT_USER" -- env HOME="$WT_HOME" \
    PATH="$WT_HOME/.local/bin:$WT_HOME/.cargo/bin:/usr/local/go/bin:/usr/local/bin:/usr/bin:/bin" \
    bash -o pipefail -c '
        set -eu
        curl -LsSf https://astral.sh/uv/install.sh |
            env UV_INSTALL_DIR="$HOME/.local/bin" sh
        "$HOME/.local/bin/uv" python install --default
    '

phase "configuring Node.js command path"
node_bin=$(runuser --user "$WT_USER" -- env HOME="$WT_HOME" \
    PATH="$WT_HOME/.local/bin:$WT_HOME/.cargo/bin:/usr/local/go/bin:/usr/local/bin:/usr/bin:/bin" \
    bash -o pipefail -c '
        . "$HOME/.nvm/nvm.sh"
        dirname "$(command -v node)"
    ')
for tool in node npm npx corepack; do
    test -x "$node_bin/$tool"
    ln -sfn "$node_bin/$tool" "/usr/local/bin/$tool"
done

phase "configuring Docker for the retained-world user"
usermod --append --groups docker "$WT_USER"
cat >> "$WT_HOME/.bashrc" <<'EOF'

# WT toolchain
export PATH="$HOME/.local/bin:$HOME/.cargo/bin:/usr/local/go/bin:$PATH"
export NVM_DIR="$HOME/.nvm"
[ -s "$NVM_DIR/nvm.sh" ] && . "$NVM_DIR/nvm.sh"
if [[ $- == *i* && -z ${WT_TOOLCHAIN_ANNOUNCED:-} ]]; then
    export WT_TOOLCHAIN_ANNOUNCED=1
    printf '%s\n' 'WT toolchain: Rust/Cargo, Go, Python/uv, Node.js/npm (via nvm), build tools, CLI utilities, and Docker/Compose.'
fi
EOF
chown "$WT_USER:$WT_GROUP" "$WT_HOME/.bashrc"

phase "recording installed development-tool versions"
runuser --user "$WT_USER" -- env HOME="$WT_HOME" \
    PATH="$WT_HOME/.local/bin:$WT_HOME/.cargo/bin:/usr/local/go/bin:/usr/local/bin:/usr/bin:/bin" \
    bash -o pipefail -c '
        set -eu
        command -v cargo rustc go python node npm npx corepack uv docker shellcheck
        . "$HOME/.nvm/nvm.sh"
        command -v nvm
        {
            printf "cargo\t%s\n" "$(cargo --version)"
            printf "rustc\t%s\n" "$(rustc --version)"
            printf "go\t%s\n" "$(go version)"
            printf "python\t%s\n" "$(python --version)"
            printf "nvm\t%s\n" "$(nvm --version)"
            printf "node\t%s\n" "$(node --version)"
            printf "npm\t%s\n" "$(npm --version)"
            printf "npx\t%s\n" "$(npx --version)"
            printf "corepack\t%s\n" "$(corepack --version)"
            printf "shellcheck\t%s\n" "$(shellcheck --version | grep -E "^version:" | cut -d: -f2 | tr -d " ")"
            printf "uv\t%s\n" "$(uv --version)"
            printf "docker\t%s\n" "$(docker --version)"
            printf "docker-compose\t%s\n" "$(docker compose version)"
        }
    ' > /var/lib/wt-image-development-tools
chown root:root /var/lib/wt-image-development-tools
chmod 0644 /var/lib/wt-image-development-tools
systemctl enable docker.service docker.socket

rm -f "$go_release" "/var/tmp/$go_archive"
