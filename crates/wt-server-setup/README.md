# wt-server-setup

Ubuntu 24.04 amd64 server installer. Source-checkout tool. Not part of `wt-server`.

```text
wt-server-setup validate --config PATH
wt-server-setup install --config PATH
wt-server-setup image build --config PATH
wt-server-setup image rebuild --config PATH
```

Owns host validation, strict server config, KVM golden image construction, provenance checks, and binary installation.

The implementation follows those responsibilities:

- `main.rs` parses commands and dispatches them.
- `server.rs` orchestrates installation and enforces the complete server config.
- `host.rs` validates and prepares Ubuntu, KVM, libvirt, and server directories.
- `image.rs` builds, verifies, and publishes the golden image and its provenance.
- `registry_cache.rs` installs and verifies the shared container registry cache.
- `files.rs` contains strict ownership/mode checks and privileged file publication.
- `runner.rs` is the small command-execution boundary used by the other modules.

The configured worlds directory is strict server state: server user:`kvm`, mode
`2770`, plus `user:libvirt-qemu:--x`. Temporary image-build directories use
the same ACL. This gives QEMU traversal without granting access to other local
users and prevents virt-install path-search warnings.

The installer runs a pinned registry-proxy container on the libvirt bridge,
installs its CA, and configures host Docker to use it. The cache size and public
registry hosts are part of the strict server configuration.

Golden-image builds stream the temporary guest's serial console, including
cloud-init package output, phase timings, and quiet-period heartbeats. A matching
installed image is verified and reused without starting the build guest.

Production instructions: [`wt-server` Install on Ubuntu](../wt-server/README.md#install-on-ubuntu).
