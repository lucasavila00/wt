# wt-integration-tests

Cross-crate integration tests. No production code.

| Lane | Backend |
|------|---------|
| Injected | Test-only `WorldWorker` implementation |
| Real system | Production `wt-libvirt` against local libvirt/KVM |

The real-system lane always runs. Local setup and commands are documented in
[DEVELOPMENT.md](../../DEVELOPMENT.md).

## Registry cache

The real-system test uses the production golden image and the host registry
cache installed by `scripts/install-server`. Each test world keeps an independent
Docker daemon and qcow2 overlay while image blobs are served by the shared
network cache.
