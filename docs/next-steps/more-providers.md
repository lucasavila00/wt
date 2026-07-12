# More providers

Keep one VM per world. Keep full Docker Compose support, including host
networking.

Split provisioning in two:

1. Machine provider creates, inspects, and deletes a VM.
2. Guest provisioner configures the VM and starts the devcontainer.

```text
wt-server
  +-- machine provider
  |     create / inspect / delete
  |     returns SSH endpoint and host keys
  |
  +-- guest provisioner
        install or verify Docker, Compose, and Dev Container CLI
        clone repository
        start devcontainer
        install WT helpers
        verify guest and app SSH
```

## Machine provider contract

Return a clean supported Linux VM with:

- SSH
- Pinned host keys
- Root or sudo access
- Requested CPU, memory, and disk

The provider owns:

- VM lifecycle
- Provider resource ID
- Network discovery
- VM image selection

WT owns everything inside the VM.

Use SSH as the common guest transport. Libvirt may use the QEMU guest agent for
bootstrap and readiness checks. Shared guest provisioning must not require a
libvirt domain.

## Order

1. Move provider-neutral lifecycle types out of `wt-libvirt`.
2. Extract shared guest provisioning.
3. Add command and file-transfer guest transport interfaces.
4. Convert libvirt to the machine provider interface.
5. Add an existing-VM-over-SSH provider.
6. Add a cloud VM provider.

Start with one provider per `wt-server`. The client context selects the server
and infrastructure pool.

Do not build a plugin system yet. First prove the interface with libvirt and an
existing SSH VM.

Do not add container-only or Kubernetes providers under the same compatibility
claim. They cannot guarantee the same Compose, host-networking, privileged
container, and bind-mount behavior.
