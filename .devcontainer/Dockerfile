FROM rust:1-bookworm

RUN rustup component add clippy rustfmt \
    && apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        curl \
        libvirt-dev \
        pkg-config \
        sudo \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 1001 wt \
    && useradd --uid 1001 --gid 1001 --create-home --shell /bin/bash wt \
    && printf 'wt ALL=(ALL) NOPASSWD: ALL\n' > /etc/sudoers.d/wt \
    && chmod 0440 /etc/sudoers.d/wt \
    && curl -fsSL https://chatgpt.com/codex/install.sh \
        | CODEX_INSTALL_DIR=/usr/local/bin CODEX_NON_INTERACTIVE=1 sh \
    && codex --version
