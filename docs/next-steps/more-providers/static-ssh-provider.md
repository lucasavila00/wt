# Static SSH provider

Claim one existing VM for zero or one WT world. Never create, stop, rebuild, or
delete the VM.

## Target

- dedicated Ubuntu 24.04 amd64 VM;
- root or passwordless sudo;
- pinned OpenSSH bootstrap identity;
- enough CPU, memory, and disk;
- no existing WT claim;
- no unrelated Docker state unless ownership can be proved.

KVM and the WT golden image are not required.

## Config

```toml
[backend]
kind = "static_ssh"
host = "work-vm"
identity_file = "/etc/wt/static-ssh/id"
known_hosts_file = "/etc/wt/static-ssh/known_hosts"
```

`host` is an OpenSSH destination. Use only the configured identity and pinned
known-hosts file. Disable host-key prompts, agent fallback, and ambient identity
selection.

Server config selects exactly one backend.

## Provider behavior

`create` means claim:

1. Connect with pinned OpenSSH.
2. Verify OS, architecture, privilege, resources, and machine identity.
3. Fail on an existing or malformed claim.
4. Atomically create a claim containing `backend_id` and world name.
5. Return `Machine` with OpenSSH `GuestTransport`.

Claim checks exclude Docker and development tools. Shared bootstrap installs
them.

`inspect`:

1. Connect with pinned OpenSSH.
2. Return `None` when the claim is absent.
3. Return an error for unreachable host, changed identity, malformed claim, or
   claim mismatch.
4. Return `Machine` when the claim matches.

`delete` means release claim. It runs only after world cleanup succeeds. Remove
the claim last. Missing claim is success only when no WT-owned world state
remains.

## World lifecycle

Create uses the shared provisioner unchanged:

```text
claim machine -> bootstrap -> clone -> devcontainer -> helpers -> verify SSH
```

Keep the VM's pinned SSH host identity. Do not rotate it during provisioning.
The final guest endpoint may use the same SSH server. App SSH keeps its separate
world identity.

Static deletion extends the composite lifecycle:

```text
WorldProvisioner::cleanup
  -> MachineProvider::delete
```

Cleanup removes only WT-owned state:

1. Stop and remove the world devcontainer and Compose resources.
2. Remove WT-created containers, networks, volumes, and files.
3. Remove checkout, Git material, helpers, users, and WT configuration owned by
   this world.
4. Verify no owned state remains.
5. Release claim.

Cleanup failure keeps the claim. A new world cannot use a dirty machine.

Record enough ownership metadata at create time to make cleanup exact and
retryable. Never infer ownership from broad name prefixes alone.

## Client access

The static VM may not be directly reachable from the workstation. Endpoint
routing is provider-neutral:

```text
direct address | server proxy
```

Static worlds use `wt-server proxy BACKEND_ID`. Remote contexts invoke that
through the existing context SSH connection. The workstation's inner SSH still
authenticates directly to and verifies the guest or app endpoint.

Persist the route needed to rebuild SSH inventory in the API and registry. Do
not encode it as a fake IP address.

## Failure rules

- Claim creation is atomic.
- Claim mismatch touches nothing.
- Provision failure runs cleanup, then releases claim only after clean state.
- Cleanup error stays primary; claim-release error is secondary.
- Transport never logs command stdin, key contents, or written secrets.
- Retry after interruption resumes from recorded ownership and claim state.
