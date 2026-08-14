# wt-github-ci

Library foundation for one-job GitHub Actions worlds.

It owns runner reservation, JIT configuration, job execution, reconciliation,
logs, and cleanup against the shared registry and machine provider.

This crate does not ship a runner executable, service installer, or runner
image builder yet.

Contract: [GitHub CI world foundation](../../docs/worlds/github-ci.md).
