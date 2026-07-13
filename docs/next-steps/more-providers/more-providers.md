# More providers

Keep one VM per world. Keep Docker Compose behavior, including host networking,
privileged containers, and bind mounts.

## Model

```text
wt-server
  -> machine provider
       create / inspect / delete
       returns machine + guest transport
  -> world provisioner
       bootstrap OS tools
       clone repository
       start devcontainer
       install helpers
       verify guest and app SSH
```

One backend per server.

| Crate | Role |
|-------|------|
| `wt-provider` | Neutral contracts, bootstrap, provisioning, composite lifecycle |
| `wt-libvirt` | KVM lifecycle and QEMU guest-agent transport |
| `wt-static-ssh` | Existing-machine claim and pinned OpenSSH transport |

Transport is a behavior contract, not always SSH. Shared provisioning never
accepts a libvirt domain or invokes provider APIs directly.

## Machine contract

A provider returns a supported Ubuntu 24.04 amd64 machine with:

- working privileged command and file transport;
- stable provider ID;
- current endpoint data;
- requested resources when the provider creates the VM.

The provider owns machine allocation, transport bootstrap, identity pinning,
network discovery, and machine deletion or claim release.

WT shared provisioning owns packages, users, workspace, Git, registry trust,
devcontainer, helpers, and final SSH verification. The golden image only speeds
up bootstrap.
