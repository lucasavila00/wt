# wt-integration-tests

Cross-crate tests. This package contains no production code.

| Test | Backend |
|------|---------|
| Service behavior | Injected `WorldWorker` |
| Full lifecycle | Production `wt-libvirt` and local KVM |

Tests use the production API, reservation, background job, lock, registry, log,
and recovery paths. The KVM test uses the installed golden image and registry
cache.

Run from the workspace root:

```text
cargo test --workspace
```

Host setup: [Development](../../DEVELOPMENT.md).
