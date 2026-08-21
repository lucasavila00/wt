# wt

> [!WARNING]
> **Keep Rust concurrency capped at four.** The repository-wide settings in
> [`.cargo/config.toml`](./.cargo/config.toml) limit both Cargo build jobs and
> Rust test threads. Do not remove, bypass, or raise these limits: unrestricted
> workspace test runs can consume every CPU on a shared host.

WT models devcontainer, raw Ubuntu, and GitHub CI worlds on KVM. Devcontainer
and host worlds are operator-ready; the GitHub CI service is still foundation
code.

[Development and setup](./DEVELOPMENT.md)

## Documentation

| Document | Contents |
|----------|----------|
| [Documentation](./docs/README.md) | World kinds, guides, and internals |
| [Rust workspace](./WORKSPACE.md) | Packages and build commands |
| [Development](./DEVELOPMENT.md) | Setup, examples, and checks |
| [Examples](./examples/) | Client and server configuration samples |
