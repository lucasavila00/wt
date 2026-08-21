# Publish images and provenance as one generation

The server installer publishes a golden image and its provenance manifest with
two separate moves. The image-builder lock excludes concurrent builders, but a
failure, kill, or reboot between the moves can leave a new image paired with an
old or missing manifest.

Later installer runs detect the inconsistent pair, but the running server can
observe it first. Host creation may use the image directly, while devcontainer
provisioning can fail when it reads mismatched provenance.

Publish image generations in separate directories and switch a single current
pointer atomically, or use an equivalent commit marker that the runtime checks.
Add failure injection between publication steps and verify that readers see
either the complete old generation or the complete new generation.

Relevant code:

- `crates/products/wt/server-installer/src/image/builder/provenance.rs`
- `crates/products/wt/server-installer/src/image.rs`
- `crates/products/wt/server-installer/src/image/host.rs`
- `crates/products/wt/server/src/runtime_config.rs`
