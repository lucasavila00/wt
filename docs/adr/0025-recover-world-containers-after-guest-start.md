# ADR 0025: Recover containers when a devcontainer world starts

- Status: Accepted
- Date: 2026-08-14
- Uses the user contract from
  [ADR 0014](0014-require-explicit-devcontainer-remote-user.md)

## Context

For a devcontainer world, starting its libvirt domain and inspecting it is not
enough. After an abrupt guest stop, systemd starts Docker but the devcontainer
and Compose sidecars remain stopped.

Starting the containers exposes a second recovery problem. The injected Dev
Containers SSH feature can truncate its bind-mounted
`authorized_keys/REMOTE_USER` file while the container starts. The per-world
SSH host key and session identity remain intact, but strict app SSH
authentication fails against the empty authorized-keys file.

Recovery must preserve the existing guest disk, checkout, containers, SSH
identities, and credential isolation. Normal reconciliation must remain
read-only, and a KVM test must not require real GitHub, GitLab, or developer
credentials.

## Decision

Give devcontainer start a distinct recovery path after the machine provider
boots the guest. Host start does not run this path. Normal inspection remains
read-only.

The recovery path:

1. Uses one recipe deadline for the whole operation. Waiting for Docker and
   later readiness retries do not reset that deadline.
2. Waits for the guest Docker daemon, reads every full container ID from the
   world's daemon, validates the IDs in Rust, and starts the containers. Each
   devcontainer world has a dedicated guest and Docker daemon, so this restores
   devcontainer Compose sidecars without interpreting repository-specific
   Compose configuration.
3. Repeats strict world inspection until it succeeds or the shared deadline
   expires. Inspection still verifies the guest SSH identity and setup
   completion marker before discovering the live primary devcontainer.
4. Reads and validates the primary devcontainer's current `remoteUser` and
   address from runtime metadata. Recovery does not trust a separately
   persisted username.
5. Reconstructs the app authorized keys from the durable guest
   `/home/wt/.ssh/authorized_keys` and the public half of the durable per-world
   app session identity.
6. Writes the reconstructed keys as `root:root` mode `0644` to a temporary file
   beside the destination and atomically renames it to
   `authorized_keys/REMOTE_USER` before strict app SSH verification.

Restore the authorized-keys file on every readiness attempt. The feature
startup is asynchronous and may still hold the old inode open when WT performs
the first replacement. Atomic rename prevents later writes through that old
file descriptor from emptying the replacement; retrying handles a later open
by path.

Do not persist another copy of the username or combined authorized keys. The
runtime metadata and existing durable public-key sources are authoritative,
and `/var/lib/wt-app-ssh` is owned by the guest `wt` user rather than being a
new root-only integrity boundary.

Starting a devcontainer world returns only after its existing container is
reachable with the expected host key and session identity. WT still does not
restart a stopped guest automatically.

## Verification

Unit tests validate full Docker container IDs, authorized-key construction,
and the exact atomic replacement operation, including contents, ownership,
mode, temporary path, and destination.

The real-system KVM E2E test uses a local bare Git repository and in-process
GitHub and GitLab API fixtures. It uses no real provider or developer
credentials. The test:

1. Creates and sets up a world through production libvirt and guest transport.
2. Writes and flushes workspace state.
3. Abruptly destroys the libvirt domain and reconciles it to `stopped` with the
   provider reason.
4. Calls the production Start operation.
5. Verifies the guest services, strict devcontainer SSH access, preserved
   workspace state, and Git fetch through the restarted relay.
6. Deletes the world and verifies cleanup and grant revocation.

Setup failures include the tail of the guest-held installation log so failures
before a named test phase remain diagnosable.

## Consequences

- Starting a devcontainer world recovers its environment, not only its VM.
- Compose sidecars restart with the primary devcontainer.
- Because the Docker daemon belongs to one devcontainer world, recovery starts
  all retained containers in it. A deliberately stopped or orphaned container
  also starts;
  preserving exact pre-crash running state would require additional durable
  lifecycle state.
- App SSH recovery preserves strict host-key checking and public-key
  authentication instead of weakening either check.
- A permanent recovery error is reported with the last readiness failure after
  the bounded recipe deadline.
- The KVM lifecycle can be validated without placing real provider credentials
  on the test host.

## Alternatives

### Inspect immediately after starting the domain

Rejected because Docker does not automatically start the existing
devcontainer in this configuration.

### Run `devcontainer up` again

Rejected because it can rebuild or recreate the environment. Start recovery
must resume the retained containers and disk state.

### Start only the primary container

Rejected because devcontainer recipes may use Compose sidecars that are part
of the environment.

### Use automatic container or guest restart policies

Rejected because retained worlds require an explicit `wt start`; the condition
that stopped the guest may still exist.

### Save a second username and authorized-keys backup during setup

Rejected because it creates stale duplicate state and suggests a root-owned
trust boundary under a directory owned by `wt`. The durable source keys and
validated live metadata already contain what recovery needs.

### Restore authorized keys once

Rejected because the asynchronous SSH feature startup can race a single
replacement.

### Test with real provider credentials

Rejected because world restart, scoped Git transport, and provider API request
format can all be exercised with local fixtures. Real credentials would add
risk without increasing coverage of this decision.
