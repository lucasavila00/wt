# wt-setup

Ubuntu 24.04 amd64 site installer. Source-checkout tool. Not part of `wt-local`.

```text
wt-setup validate --config PATH
wt-setup install --config PATH
wt-setup image build --config PATH
wt-setup image rebuild --config PATH
```

Owns host validation, strict site config, KVM golden image construction, provenance checks, and binary installation.

Production instructions: [`wt-local` Install on Ubuntu](../wt-local/README.md#install-on-ubuntu).
