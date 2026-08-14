# ADR 0026: Make world kinds first-class

- Status: Accepted
- Date: 2026-08-14
- Amends: [ADR 0023](0023-run-github-actions-jobs-in-ephemeral-kvm-guests.md)
  and [ADR 0024](0024-use-a-shared-guest-registry.md)

## Context

WT began with one application: named development environments built from a
repository's devcontainer. The word *world* therefore came to mean both the
common KVM isolation boundary and that one development workflow.

ADR 0023 added another application of the same KVM machinery: ephemeral GitHub
Actions runners. It deliberately kept runners separate from worlds because a
development world's interactive, persistent lifecycle is wrong for CI. ADR
0024 then shared capacity through a common guest registry while preserving
`world` and `runner` as separate top-level kinds.

That separation confuses the application with one lifecycle policy. The useful
WT abstraction is a resource-bounded KVM environment with an explicit purpose,
image, provisioning contract, credentials, interfaces, and cleanup policy.
Interactive development and one-job CI are different kinds of that same
abstraction.

WT also needs a minimal interactive environment whose application boundary is
the Ubuntu guest itself. Users need a named, retained KVM machine without a Git
checkout, agent Git grant, devcontainer, app SSH server, or repository setup.
Calling that environment a devcontainer world with most fields disabled would
make invalid states easy to construct and would keep the current terminology
problem.

## Decision

Make **world** the first-class application abstraction above a retained KVM
guest. Every world has an explicit, immutable kind. Initially WT supports:

| Kind | Purpose | Owner and lifetime | Application boundary |
|------|---------|--------------------|----------------------|
| `devcontainer` | Repository development | User-owned, named, retained | Primary devcontainer |
| `host` | Raw Ubuntu work | User-owned, named, retained | Ubuntu guest |
| `github-ci` | One GitHub Actions job | Service-owned, ephemeral | Actions runner process |

`host` refers to the world guest, not the physical WT server.

World kind is stored in the registry, returned by APIs, shown in inventory, and
used for typed lifecycle dispatch. Do not infer it from the presence or absence
of Git, runner, or SSH fields. Do not represent the three requests as one
structure with many optional fields.

A world's kind cannot change in place. Changing purpose requires creating a
new world and deleting the old one. This keeps credentials, images, lifecycle
state, and cleanup rules tied to the decision made at creation.

## Common world contract

All world kinds use the common WT infrastructure for:

- a KVM guest and copy-on-write disk identity;
- CPU, memory, and disk admission in the shared registry;
- an explicit image and network policy;
- typed creation, inspection, failure, and deletion state;
- cleanup that retains the capacity reservation until machine and disk cleanup
  succeeds; and
- reconciliation after service or host restart.

This common contract does not imply one user interface or one lifecycle. A
kind advertises capabilities, and WT rejects unsupported operations rather than
silently approximating them.

## Devcontainer worlds

The current development world becomes the `devcontainer` kind. Its contract is
unchanged:

- It is named, interactive, retained across sessions, and explicitly started
  after an unexpected stop.
- Creation requires a Git source, base revision, Git author, resource request,
  and workstation-authorized SSH keys.
- It receives a project- and namespace-scoped agent Git grant, clones into
  `/workspace`, and uses an `ag::` remote. Provider credentials never enter the
  guest.
- It installs and starts the repository's devcontainer, requires an explicit
  `remoteUser`, and exposes guest and app SSH identities.
- Its normal shell and editor interfaces enter the primary devcontainer.
- Start recovery restores the retained containers and verifies app SSH as
  defined by ADR 0025.

Operations whose meaning depends on an application container, including the
current editor flow, are available only for this kind.

## Host worlds

Add the `host` kind for an interactive, retained Ubuntu environment. It uses
the same naming, resource admission, KVM isolation, managed guest identity,
stop/start recovery, and deletion model as a devcontainer world, but stops at
guest readiness.

A host world contains only the WT-managed Ubuntu guest facilities required to
operate it, including the QEMU guest agent, OpenSSH, a managed login account,
authorized workstation keys, and unique machine and SSH identities. *Raw
Ubuntu* means there is no repository or application layer; it does not mean an
unmanaged VM.

A host world:

- accepts no Git source, base revision, or Git author;
- receives no agent Git gateway grant, provider token, Git SSH key, or
  repository credential;
- does not clone or create `/workspace` as a WT checkout;
- does not install the agent Git relay or `ag-git` tools;
- does not read or start a devcontainer or Compose application;
- has no app SSH identity, app proxy, `NAME-vs` endpoint, or editor operation;
  and
- opens its normal interactive connection directly in the Ubuntu guest with no
  forced devcontainer session command.

Do not implement a host world by running the devcontainer provisioner with an
empty repository. Give it a small, typed guest-readiness provisioner so its
absence of Git and app state is enforced rather than conventional.

## GitHub CI worlds

The existing ephemeral runner design becomes the `github-ci` world kind. This
changes its taxonomy, not its isolation or one-job lifecycle.

`wt-runner` continues to own GitHub App authentication, the scale-set protocol,
JIT configuration, runner image, and automatic cleanup. A GitHub CI world:

- is service-owned and identified by internal and GitHub job or runner
  identities rather than a reusable developer-selected name;
- uses the dedicated runner image and runner network;
- runs exactly one official GitHub Actions runner for one job;
- has no interactive SSH inventory, workstation-authorized keys, Byobu,
  devcontainer, app SSH identity, or agent Git grant;
- receives only the short-lived JIT material needed by its runner; and
- is destroyed after success, failure, cancellation, timeout, runner loss, or
  service recovery. It is never started again, forked, or reused.

GitHub remains authoritative for workflow state, logs, artifacts, and
cancellation. Calling the guest a world does not turn WT into a second CI user
interface or log store.

## Ownership and services

Keep lifecycle ownership separated by application policy:

- `wt-server` owns user-created `devcontainer` and `host` worlds.
- `wt-runner` owns service-created `github-ci` worlds.
- `wt-registry`, `wt-libvirt`, image setup, and capacity admission remain shared
  infrastructure.

The registry keeps `guests` as the machine, disk, and capacity record. Replace
the parallel top-level development-world and runner taxonomy with a common
world record containing its kind and owner class. Store kind-specific state in
typed one-to-one records:

- devcontainer source, grant, setup, guest SSH, and app SSH state;
- GitHub scale-set, job, runner, and diagnostic state; and
- no application subtype for a host world beyond its common guest SSH and
  lifecycle state.

Do not force every kind into one lifecycle enum. Common machine cleanup and
capacity state belong to the guest record; application lifecycle remains typed
by world kind.

## Client and API behavior

Interactive creation makes the world kind explicit. The devcontainer kind may
remain the default for the existing `wt new` flow, but the create request sent
to the server is a tagged kind-specific request. Selecting `host` removes the
Git and devcontainer questions rather than accepting empty answers.

GitHub CI worlds are created only by `wt-runner` in response to the scale set,
not through the interactive create flow.

Inventory represents the kind of every world. User-facing lists may separate
retained interactive worlds from rapidly changing CI worlds, but operator and
diagnostic views must use the same world identity and kind model. Kind-specific
commands fail with a typed unsupported-operation error.

The registry shape changes again and WT remains pre-release. Before installing
this change, run `make nuke`. It destroys all existing WT guests and world
disks, deletes the SQLite registry and installed configuration, and requires a
clean reinstall. No existing world or database state is preserved, and no
in-place schema migration from ADR 0024's `world` versus `runner` model is
provided.

Keep the client/server protocol at version 1. This reset replaces the version-1
request, response, and registry definitions in place. WT's exact client/server
commit check rejects mixed binaries, so a protocol-version bump or compatibility
shape would add no useful protection.

## Credential boundaries

The kind determines the only credentials that may enter a guest:

| Kind | Guest credential material |
|------|---------------------------|
| `devcontainer` | Workstation-authorized public keys and one scoped agent Git grant |
| `host` | Workstation-authorized public keys only |
| `github-ci` | One job's short-lived GitHub JIT runner material only |

Host and devcontainer worlds never receive the GitHub App private key. GitHub
CI worlds never receive developer SSH keys, agent Git grants, or provider Git
credentials. The host world's lack of WT-provided repository credentials is a
security property, not only an omitted convenience.

## Verification

Each kind needs a real KVM lifecycle test at its actual application boundary:

- `devcontainer`: create through production libvirt, complete setup through
  local Git and provider API fixtures, verify app SSH and scoped Git, force a
  stop, start it, and delete it.
- `host`: create without Git inputs or grants, verify direct Ubuntu SSH and the
  absence of workspace, agent Git, and app endpoints, force a stop, start it,
  and delete it.
- `github-ci`: exercise reservation, guest launch, one-runner completion, and
  unconditional cleanup with local scale-set and runner-process fixtures.

These tests must not use a developer's real SSH private key, Git provider token,
or GitHub App credential. Protocol contract tests and local fixtures cover the
control plane; an operator may run a separate opt-in live GitHub smoke test,
but it is not required for repository verification.

Cross-kind tests verify atomic shared capacity, typed operation rejection,
credential isolation, service-restart reconciliation, and cleanup of every
guest and disk.

## Consequences

- WT becomes an application platform for multiple isolated KVM world kinds
  instead of a devcontainer tool with a separate runner subsystem.
- The common world model makes capacity, inventory, cleanup, and diagnostics
  consistent without erasing lifecycle differences.
- Host worlds provide the smallest useful interactive WT environment and avoid
  unnecessary Git, Docker, and devcontainer setup.
- GitHub CI keeps fresh-VM, one-job isolation while becoming visible to common
  world accounting and diagnostics.
- APIs, registry records, client presentation, setup, and documentation must
  stop using unqualified *world* as a synonym for *devcontainer world*.
- Every generic operation needs an explicit capability decision for each kind.
- Existing installations are intentionally destroyed with `make nuke`; the
  SQLite registry is recreated from an empty version-1 schema.

## Alternatives

### Keep runners separate from worlds

Rejected because it duplicates the top-level application model and makes
inventory, diagnostics, and future world kinds branch between two taxonomies.
The CI lifecycle remains separate even when the abstraction is shared.

### Treat a host world as an empty devcontainer world

Rejected because it creates meaningless optional Git and app state, risks
granting credentials the host world must not receive, and still couples raw
Ubuntu startup to repository tooling.

### Infer kind from supplied fields

Rejected because malformed combinations become ambiguous and stored records
can change meaning as fields are added. Kind is an explicit tagged decision.

### Use one image and one provisioning script for every kind

Rejected because the CI image contains the official runner, devcontainer
worlds need repository application tooling, and host worlds intentionally need
neither. Shared image-building primitives do not require identical images or
guest payloads.

### Give every world the same lifecycle and commands

Rejected because named interactive environments can be restarted and retained,
while a CI world must be anonymous, single-use, and unconditionally destroyed.
Capabilities are shared explicitly, not assumed from the word *world*.

### Allow in-place kind conversion

Rejected because credentials, image contents, interfaces, and cleanup policy
would cross trust boundaries. Create a new world of the desired kind instead.
