# wt-integration-tests

Cross-crate integration tests. No production code.

| Lane | Backend |
|------|---------|
| Injected | Test-only `WorldWorker` implementation |
| Real system | Production `wt-libvirt` against local libvirt/KVM |

The real-system lane always runs. Requirements and command: [TESTS.md](../../TESTS.md).

## KVM image cache

The real-system test keeps its complete lifecycle coverage but uses a separate
cached backing image prepared with:

```text
scripts/prepare-test-image --config config/wt-server.development.toml
```

Run it from the repository root in an interactive terminal. It invokes `sudo`.
Re-run it after rebuilding the production golden image or changing
`fixture-images.txt`.

The cache contains the exact container images listed in `fixture-images.txt`.
Test worlds remain disposable qcow2 overlays. The test refuses a cache whose
production-image digest or fixture image list has drifted, rather than silently
falling back to slow network pulls. The cache consumes additional disk space
next to the production golden image and survives normal test cleanup.
