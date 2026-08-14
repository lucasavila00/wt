# Provider boundaries

`wt-provider` contains only backend-neutral machine and guest-transport
contracts:

- `MachineSpec`, resources, disk identity, and `NoCloudConfig`;
- create, fork, inspect, start, and delete operations;
- bounded guest command, capture, and file-write transport.

`wt-libvirt` implements those contracts with libvirt/KVM and the QEMU guest
agent. It does not know repository, host-recipe, or GitHub job semantics.

Kind crates compose machines into applications:

- `wt-devcontainer` owns checkout, bootstrap, Docker/devcontainer, app SSH, and
  restart recovery;
- `wt-host` owns vendor-data, cloud-init readiness, one-use login proof and key
  removal, and direct SSH;
- `wt-github-ci` defines JIT runner execution and one-job cleanup; its operator
  process is not shipped yet.

`wt-server` dispatches retained operations by the stored kind. It never sends a
host through the devcontainer worker. A future `wt-runner` process will use the
CI worker directly.

This boundary keeps shared provider code free of kind-specific options and
prevents empty or optional fields from standing in for a typed application.
