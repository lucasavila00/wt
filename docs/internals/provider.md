# Provider boundaries

`wt-libvirt-kvm` owns the machine and guest-transport contracts together with
the only supported production implementation:

- `MachineSpec`, resources, disk identity, and `NoCloudConfig`;
- create, inspect, start, stop, disk usage, and delete operations;
- bounded guest command, capture, and file-write transport.

It uses libvirt/KVM and the QEMU guest agent and does not know repository,
host-recipe, or GitHub job semantics. A separate provider abstraction is not
maintained until a second production provider exists.

Kind crates compose machines into applications:

- `wt-retained-worlds::devcontainer` owns checkout, bootstrap, Docker/devcontainer, app SSH, and
  restart recovery;
- `wt-retained-worlds::host` owns QGA login preparation, root-only recipe staging, cloud-init
  setup state, one-use login proof and key removal, and direct SSH;
- `wt-gh-actions-runner` defines JIT runner execution and one-job cleanup; its operator
  process is not shipped yet.

`wt-server` dispatches retained operations by the stored kind. It never sends a
host through the devcontainer worker. A future `wt-runner` process will use the
CI worker directly.

This boundary keeps shared provider code free of kind-specific options and
prevents empty or optional fields from standing in for a typed application.
