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

## Reset an older development installation

The site config and image provenance are intentionally strict and have no development-schema migrations. After either format changes, an older installation can fail with errors such as a missing manifest field or config drift.

After removing any existing worlds, delete the installed config, golden image, and its adjacent manifest together, then reinstall:

```bash
sudo rm -f \
  /etc/wt/local.toml \
  /var/lib/wt/images/wt-ubuntu-24.04-amd64.qcow2 \
  /var/lib/wt/images/wt-ubuntu-24.04-amd64.qcow2.manifest.json

scripts/install-site --config config/wt-local.development.toml
```

The cached Ubuntu source image under `imgs/` does not need to be removed. Setup verifies and reuses it while rebuilding the golden image and manifest.

Production instructions: [`wt-local` Install on Ubuntu](../wt-local/README.md#install-on-ubuntu).
