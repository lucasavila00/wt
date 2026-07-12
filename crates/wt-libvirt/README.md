# wt-libvirt

Production libvirt/KVM backend.

## Owns

- Domain, network, qcow2 overlay, and cloud-init lifecycle.
- Guest-agent readiness and file injection.
- SSH Git clone, revision checkout, and checkout-local credentials.
- Stock devcontainer startup and app SSH injection.
- Guest and app identity verification.

Create succeeds only after guest SSH, Git, and app SSH are ready. Failure removes
the domain and world directory.

Lifecycle: [Libvirt/KVM backend](../../docs/how/bare-metal-agent.md).
