FROM mcr.microsoft.com/devcontainers/rust:2-1-bookworm

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        libvirt-dev \
        pkg-config \
    && rm -rf /var/lib/apt/lists/*
