# Run universal-image KVM E2E on a suitable host

Run one cold and one warm `make e2e-tests` with the universal development-tools
image. This WT world has two vCPUs, so do not build the golden image here.

Confirm the first run creates `imgs/wt-development-tools.qcow2` and the second
reuses it while E2E validates the installed toolchain.
