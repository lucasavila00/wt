# ADR 0043: Split fast KVM tests from full lifecycle coverage

- Status: Accepted
- Date: 2026-08-20

## Context

WT's ignored KVM E2E currently proves many unrelated contracts in one serial
test. It builds a Rust devcontainer, installs the complete host development
recipe, runs workspace Clippy inside the host, exercises the agent Git gateway,
restarts both world kinds, and creates two more hosts for failure recovery.

That coverage is valuable, but a successful run currently takes more than seven
minutes on the development KVM server. A change to VM attachment or guest
mounting cannot get a real-system result without paying for the host toolchain,
provider API, and failure-path checks. Failures late in the flow make iteration
especially slow.

Unit and shell tests should continue to reject most defects before KVM starts.
The remaining real-system test should be proportional to the behavior being
changed.

## Decision

Create two ignored KVM profiles.

The fast profile uses the production server, libvirt provider, guest transport,
and retained-world provisioners with isolated overlays, but keeps its fixtures
small:

- one minimal Docker Compose devcontainer with a non-root `wt` user;
- one host with minimal cloud-init user-data;
- only the local Git and gateway behavior required to create those worlds;
- focused assertions for VM devices, guest mounts, Compose bind mounts,
  stop/start recovery, cross-world data, and deletion persistence.

It does not install the example host development environment, run Cargo inside
a guest, exercise provider APIs, or create intentional failure worlds. The fast
profile is the normal KVM development loop and is exposed as
`make e2e-tests-fast`.

The full profile retains the comprehensive lifecycle test. It continues to
verify the checked-in host recipe, development tools, workspace Clippy, agent
Git policy and provider APIs, persistent application state, restart recovery,
and failed or interrupted host setup. Run it before release and for changes to
those whole-flow contracts with `make e2e-tests-full`. The existing
`make e2e-tests` command remains an alias for this profile.

Both profiles remain serialized by the KVM test lock and use disposable image
overlays. Do not make them faster by sharing mutable world disks, Docker state,
registries, databases, or guest setup state between tests. Cached downloads are
allowed, but correctness must not depend on a warm cache.

Keep phase timings visible for both profiles. Treat their durations as
diagnostics rather than hard test assertions because KVM hosts and networks
vary.

## Consequences

- VM, virtiofs, and guest-mount changes get real-system feedback without the
  full host toolchain and failure-recovery cost.
- The comprehensive test remains available instead of losing broad integration
  coverage to improve local iteration time.
- The fast and full fixtures must state their separate contracts clearly so
  coverage does not silently move out of both.
- Some KVM setup remains unavoidable: both world kinds must boot, and a real
  Compose container must start to prove the documented bind mounts.
- Adding a new KVM assertion requires choosing the cheapest profile that
  exercises its production boundary; it should not default to the full flow.
- On the development KVM server, the acceptance runs took 211 seconds for the
  fast profile and 434 seconds for the full profile. These are observations,
  not duration requirements.
