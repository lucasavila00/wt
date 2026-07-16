# ADR 0004: Make world creation interactive and request-owned

- Status: Accepted
- Date: 2026-07-15

## Context

`wt new SOURCE NAME` accepts the repository and name. CPU, RAM, disk, and SSH
authorized keys come from `/etc/wt/server.toml`.

Those values describe a world, not the server. The result is a leaky boundary:

- every world gets one global CPU, RAM, and disk size;
- changing a world size requires server configuration access;
- guest access depends on a public-key file on the server;
- the server operator must copy client public keys onto the server;
- a remote `wt new` uses keys from the server machine, not the workstation that
  requested the world;
- the create request does not contain all inputs needed to reproduce or retry
  the world.

## Decision

Make world inputs client-owned. Send them in the create request.

### Interactive `wt new`

`wt new` is an interactive prompt flow:

1. Context, when more than one context exists.
2. World name.
3. Git repository.
4. Git branch or ref.
5. Virtual CPUs.
6. RAM in MiB.
7. Disk size in GiB.
8. Confirmation of the complete request.

World name and Git repository are required and have no defaults. An empty
answer asks the question again.

Show defaults for context, branch or ref, CPU, RAM, disk, and confirmation. An
empty answer accepts the displayed default. Use no branch or ref as the
checkout default, the first configured context, and documented client resource
defaults. Confirmation defaults to yes.

Validate each answer before continuing. Ask again when an answer is invalid.
Do not contact the server until the complete request is confirmed.

Remove positional create arguments and create flags. Reject `wt new` when stdin
or stderr is not a terminal.

### Interruption and exit

The prompt UI may use raw terminal mode while collecting input. It must restore
the terminal before returning. EOF or Escape cancels creation and exits without
contacting the server.

Handle `SIGINT`, `SIGTERM`, and `SIGHUP` as cancellation while prompting.
Restore terminal state, print a short cancellation message, and exit nonzero.
Do not print a partially entered value.

`SIGKILL` cannot be caught or handled. Keep no required client cleanup and make
the prompt phase mutation-free, so `SIGKILL` before confirmation leaves no
world or client state. After confirmation, the server owns the create
operation. If the client is killed during that request, `wt ls` reports the
world and an identical retry follows the existing idempotent create rules.

### Client-owned SSH keys

Before creation, the client scans `~/.ssh/*.pub` on the machine running
`wt new`.

- Read regular public-key files only.
- Parse and validate supported OpenSSH public keys.
- Remove duplicates.
- Fail before contacting the server when no valid key exists.
- Show the number and fingerprints of selected keys in the confirmation.
- Send the public keys in the create request.

Do not read private keys. Do not upload agent contents. Agent forwarding remains
separate and supplies credentials only while an SSH connection is active.

### Create request

Add these fields to `CreateInstance`:

- `vcpus`
- `memory_mib`
- `disk_gib`
- `ssh_authorized_keys`

The existing repository, revision, Git author, and world name remain request
inputs. Context selection chooses the server before sending the request.

Rust code on the client and server validates resource values and public keys.
The server includes resources and authorized keys in the exact create-input
fingerprint. Retrying an existing world succeeds only when every input matches.

WT has not shipped. Replace protocol version 1 in place. Do not add a
compatibility path.

### Server configuration

Remove these fields from `guest` in `/etc/wt/server.toml`:

- `memory_mib`
- `vcpus`
- `disk_gib`
- `ssh_authorized_keys_file`

The server config keeps server-owned infrastructure and policy: image paths,
libvirt network and storage, registry cache, Git host trust, install paths, and
timeouts.

Server setup must not inspect the installing user's `~/.ssh` directory. The
server receives guest authorized keys only through each create request.

If resource limits are needed later, add explicit server policy limits. Do not
use one global world size as both a default and a limit.

## Verification

- Interactive `wt new` prompts for every missing world input.
- World name and repository reject empty answers.
- Pressing Enter accepts every other displayed default.
- Invalid answers are asked again on the same line.
- Non-interactive creation fails before contacting the server.
- EOF and catchable termination signals exit without creating a world.
- `SIGKILL` during prompting leaves no client or server state.
- Killing the client after confirmation does not cancel or orphan the
  server-owned create operation.
- Invalid and zero resource values fail before the request.
- Creation fails when the client has no valid `~/.ssh/*.pub` keys.
- Multiple valid public keys are deduplicated and installed in the guest and
  devcontainer.
- A remote context uses keys from the client workstation, not the WT server.
- Different resources or keys make an existing-world retry conflict.
- Server config rejects removed world-owned fields as unknown.
- Real-system creation applies the requested CPU, RAM, disk, and SSH keys.

## Consequences

- `wt new` becomes the complete world-creation interface.
- Different worlds can have different resource sizes.
- The requesting workstation controls access to the created world.
- Server configuration no longer contains per-world defaults or client key
  paths.
- Create requests and registry fingerprints become larger because they contain
  public keys.
- Users with many public keys authorize all of them. They must remove unwanted
  `.pub` files before creation.
- `wt new` cannot be used from scripts or other non-interactive callers.

## Alternatives

### Keep resource defaults and SSH keys in server config

Rejected. It keeps world inputs outside the create request and breaks remote
client ownership.

### Read only `~/.ssh/id_ed25519.pub`

Rejected. It assumes one key type and one filename.

### Read keys from `ssh-agent`

Rejected. The agent may be unavailable, may expose only a subset of persistent
access keys, and is intended for signing rather than key-file discovery.

### Prompt for paths to public keys

Rejected as the default. It adds work for the common case. Automatic discovery
is deterministic and the confirmation shows what will be authorized.

### Keep command-line create arguments

Rejected for now. One interactive path is enough. A non-interactive interface
can be designed when a real automation use case exists.
