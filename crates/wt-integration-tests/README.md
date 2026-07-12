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

The workspace command skips the ignored full-lifecycle test. Run that test on a
configured Ubuntu/KVM host with:

```text
make e2e-tests
```

Host setup: [Development](../../DEVELOPMENT.md).
