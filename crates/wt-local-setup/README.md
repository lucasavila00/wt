# wt-local-setup

Ubuntu 24.04 amd64 site installer. Source-checkout tool. Not part of `wt-local`.

```text
wt-local-setup validate --config PATH
wt-local-setup install --config PATH
wt-local-setup image build --config PATH
wt-local-setup image rebuild --config PATH
```

Owns host validation, strict site config, KVM golden image construction, provenance checks, and binary installation.

The implementation follows those responsibilities:

- `main.rs` parses commands and dispatches them.
- `site.rs` orchestrates installation and enforces the complete site config.
- `host.rs` validates and prepares Ubuntu, KVM, libvirt, and site directories.
- `image.rs` builds, verifies, and publishes the golden image and its provenance.
- `files.rs` contains strict ownership/mode checks and privileged file publication.
- `runner.rs` is the small command-execution boundary used by the other modules.

Production instructions: [`wt-local` Install on Ubuntu](../wt-local/README.md#install-on-ubuntu).
