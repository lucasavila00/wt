# Run universal-image KVM E2E on a suitable host

Run one cold and one warm `make e2e-tests` with the universal development-tools
image in a WT world that satisfies the nested KVM E2E host precheck.

Confirm the first run creates `imgs/wt-development-tools.qcow2` and the second
reuses it while E2E validates the installed toolchain.
