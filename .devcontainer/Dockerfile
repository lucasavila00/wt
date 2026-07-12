FROM rust:1-bookworm

RUN rustup component add clippy rustfmt \
    && apt-get update \
    && apt-get install -y --no-install-recommends \
        libvirt-dev \
        pkg-config \
    && rm -rf /var/lib/apt/lists/*
