# Static SSH provider

Use one existing Linux VM as a WT host.

No cloud API. No VM create or delete. WT only needs SSH access.

The VM holds zero or one world.

## Config

```toml
[backend]
kind = "static_ssh"
host = "work-vm"
```

`host` is an OpenSSH destination.

## `wt new`

1. Connect over SSH with strict host-key checking.
2. Verify supported Linux, root or sudo, disk space, and required tools.
3. Check the WT claim marker.
4. Fail if the VM already holds a world.
5. Write the claim marker.
6. Install or verify Docker, Compose, and Dev Container CLI.
7. Clone the repository.
8. Start the devcontainer.
9. Install WT helpers.
10. Verify guest and app SSH.

The claim marker stores the world ID and name. Creating the marker must be
atomic.

## `wt rm`

1. Verify the claim matches the stored world ID.
2. Stop and remove the devcontainer and Compose resources.
3. Remove WT-created containers, networks, volumes, files, and checkout.
4. Remove the claim marker last.

Do not delete, stop, or rebuild the VM.

If cleanup fails, keep the claim. A new world must not reuse a dirty VM.

## Rules

- Zero or one world per VM.
- WT must not remove resources it did not create.
- Existing unrelated Docker resources make the VM invalid unless ownership can
  be proved.
- Pin the VM SSH host keys.
- Record the provider and SSH destination with the world.
- A missing or changed SSH identity is an error.
- A claim mismatch is an error.
- No Hetzner, AWS, or other infrastructure credentials are required.

This provider proves that guest provisioning works without libvirt. A later
cloud provider can create a VM, then use the same SSH provisioning path.
