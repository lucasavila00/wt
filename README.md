# wt

> [!WARNING]
> **Keep Rust concurrency capped at four.** The repository-wide settings in
> [`.cargo/config.toml`](./.cargo/config.toml) limit both Cargo build jobs and
> Rust test threads. Do not remove, bypass, or raise these limits: unrestricted
> workspace test runs can consume every CPU on a shared host.

WT runs retained Ubuntu worlds on KVM. Each world boots from a verified golden
image with persistent storage, managed SSH, Codex integration, and scoped Git
access.

[Development and setup](./DEVELOPMENT.md)

## Documentation

| Document | Contents |
|----------|----------|
| [Documentation](./docs/README.md) | User guides and internals |
| [Rust workspace](./WORKSPACE.md) | Packages and build commands |
| [Development](./DEVELOPMENT.md) | Setup, examples, and checks |
| [Examples](./examples/) | Client and server configuration samples |
