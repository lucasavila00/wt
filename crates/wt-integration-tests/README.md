# wt-integration-tests

Cross-crate integration tests. No production code.

| Lane | Backend |
|------|---------|
| Injected | Test-only `WorldWorker` implementation |
| Real system | Production `wt-libvirt` against local libvirt/KVM |

The real-system lane always runs. Requirements and command: [TESTS.md](../../TESTS.md).
