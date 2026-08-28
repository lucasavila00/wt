# Fail before caching `virt-sys` without libvirt

On a fresh Ubuntu guest, Cargo can compile `virt-sys` before `libvirt-dev` is
installed. The crate deliberately ignores failed `pkg-config` probes so that
docs.rs can build, which leaves a successful cached artifact without libvirt
link metadata. Installing `libvirt-dev` afterward is not enough: workspace
tests fail with undefined `vir*` symbols until `cargo clean -p virt-sys` is
run. This interrupts development and CI setup repeatedly.

- Add a repository-owned, fatal prerequisite/link probe that runs before WT's
  libvirt-dependent crates can be cached successfully.
- Report the missing Ubuntu package and installation command directly instead
  of deferring the failure to the linker.
- Ensure installing `libvirt-dev` after the failed probe lets the next normal
  Cargo command succeed without a manual package clean.
- Cover the missing-package, post-install retry, and native `libvirt` plus
  `libvirt-qemu` linkage behavior in a disposable Ubuntu test.

Keep this check scoped to host-side libvirt crates; guest-only and client-only
builds should not acquire a libvirt system dependency.
