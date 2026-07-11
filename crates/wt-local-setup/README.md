# wt-local-setup

Ubuntu 24.04 amd64 site installer. Source-checkout tool. Not part of `wt-local`.

```text
wt-local-setup validate --config PATH
wt-local-setup install --config PATH
wt-local-setup image build --config PATH
wt-local-setup image rebuild --config PATH
wt-local-setup image test-cache build --config PATH
wt-local-setup image test-cache rebuild --config PATH
```

Owns host validation, strict site config, KVM golden image construction, provenance checks, and binary installation.

The implementation follows those responsibilities:

- `main.rs` parses commands and dispatches them.
- `site.rs` orchestrates installation and enforces the complete site config.
- `host.rs` validates and prepares Ubuntu, KVM, libvirt, and site directories.
- `image.rs` builds, verifies, and publishes the golden image and its provenance.
- `test_cache.rs` builds and verifies the separate container-image cache used by the real KVM integration test.
- `files.rs` contains strict ownership/mode checks and privileged file publication.
- `runner.rs` is the small command-execution boundary used by the other modules.

The configured worlds directory is strict site state: site user:`kvm`, mode
`2770`, plus `user:libvirt-qemu:--x`. Temporary image-build directories use
the same ACL. This gives QEMU traversal without granting access to other local
users and prevents virt-install path-search warnings.

## Prepare the integration-test cache

After installing the local site, prepare the separate cached backing image used
by `wt-integration-tests`:

```text
scripts/prepare-test-image --config config/wt-local.development.toml
```

The wrapper performs an explicit rebuild. Use `image test-cache build` directly
to verify and reuse matching cache state. Rebuild after the production golden
image or `crates/wt-integration-tests/fixture-images.txt` changes. Both image
and manifest are published next to the production golden image; ordinary worlds
continue to use the production image.

Production instructions: [`wt-local` Install on Ubuntu](../wt-local/README.md#install-on-ubuntu).
