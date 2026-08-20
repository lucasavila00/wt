# ADR 0027: Build world images in KVM

- Status: Accepted
- Date: 2026-08-14
- Amended by: [ADR 0043](0043-own-retained-guest-foundation-in-shared-images.md)

## Context

The devcontainer image already installs packages in a temporary KVM guest. The
first host-image implementation instead ran package installation through
libguestfs and embedded the shell flow in one Rust command argument.

The clean KVM test host exposed two problems. Its libguestfs appliance did not
configure networking, so package installation could never resolve Ubuntu
repositories. The embedded command was difficult to review, syntax-check, and
diagnose; retries produced a large repeated apt transcript without naming the
failed build phase.

WT supports Ubuntu 24.04 amd64 servers with hardware KVM. The install input
pins the Ubuntu cloud image by URL and SHA-256 and names the libvirt network.
Both world images need auditable package installation there, readable recipes,
and cleanup that never hides the original failure.

## Decision

One shared builder owns disk preparation, NoCloud seeds, domains, progress,
timeouts, shutdown, offline sanitization, provenance, publication, and cleanup
for every world image. Devcontainer and host builds boot independent copies of
the pinned Ubuntu image through that builder; neither image is derived from the
other.

Shared machine and terminal provisioning has one implementation. In particular,
one shared recipe creates the fixed `wt` image user and installs and validates
Byobu, tmux, terminfo, and terminal settings for both kinds. Kind recipes
contain only their application contract:
the devcontainer stack or the host additions from
[ADR 0026](0026-make-world-kinds-first-class.md).
Devcontainer-specific tmux settings source the shared configuration instead of
copying it. The real KVM test compares the active terminal settings in both
world kinds.

Whole image installation flows are readable `.sh` assets under
`assets/world/KIND`; shared provisioning assets live under
`assets/world/shared`. Rust stages the scripts and their pinned inputs and owns
the KVM lifecycle. It does not embed multi-step shell programs in command
arguments or duplicate lifecycle logic between kinds.

Each kind has one reserved build domain and directory: `wt-image-build` or
`wt-host-image-build`. Setup holds one exclusive host build lock before checking
or creating either name, so image builds cannot overlap. Existing state under
either exact name is a conflict. The guest attaches to the configured libvirt
network. Package installation retries transient apt or DNS failures. The whole
build is bounded by 30 minutes.

The recipe writes `/var/lib/wt-image-result` as root mode `0644` only after all
installation and validation succeeds. It contains exactly these five fields:

```text
kind=KIND
status=ready
recipe_version=1
wt_uid=1001
wt_gid=1001
```

The compatibility field stays at `1`; the marker also records the fixed image
user contract. Staged-input hashes detect recipe drift. Setup requires the
exact marker before treating guest shutdown as success. It then runs
`cloud-init clean` offline, removes cached seed and generated network state,
clears machine identity and SSH host keys, and revalidates the cleaned state
plus required package versions and asset checksums.

The installed manifest records the base-image SHA, recipe version, install
configuration digest, and SHA-256 of every staged script, configuration file,
and pinned artifact. It also records retained Ubuntu package versions and the
finalized image SHA-256. Build-only packages are removed before acceptance. A
changed input cannot reuse an old image, and the manifest cannot validate a
different image.

Retained contract packages other than pinned artifacts resolve from the Ubuntu
repositories at build time. Their exact versions are recorded and validated;
two fresh builds are not promised to be byte-identical.

Image replacement does not migrate existing retained-world disks. Existing
worlds keep their independent guest users and terminal state; recreate worlds
when a changed shared image foundation is required.

Scripts write `WT_IMAGE_PHASE=TEXT` lines to the serial console. Setup prints a
heartbeat with the last phase at least once per minute. Package and download
loops report each attempt and show only the final command output when they give
up. A failure includes the last 500 console lines. The full console is copied
beside the build directory and its path is printed.

Failure cleanup first stops and undefines the exact reserved build domain. Only
after that succeeds may it remove that build's temporary seed, disk, and
directory. It never removes an installed image. Cleanup continues across
independent safe steps; any cleanup failure is appended without replacing the
primary error.

After a successful build is undefined and finalized, publishing copies the disk
and manifest to sibling temporary files on the destination filesystem. Setup
then removes the build directory. Only if that cleanup succeeds does it rename
the destination temporaries into place. The two-file publication is not atomic:
an interruption may leave a partial pair. Abandoned temporaries, a missing pair,
an output SHA mismatch, or an input mismatch are drift and fail closed on the
next install; reset removes that state before retry.

Libguestfs may stage and inspect files and clear identity while the disk is
offline. Package installation and recipe execution happen only in KVM and do
not depend on libguestfs networking.

The first combined KVM E2E used a 16 GiB overlay on a 32 GiB image. The truncated
partition table sent Ubuntu to initramfs, while repeated agent polls hid the
cause. The provider now rejects undersized disks before creation and reports
the domain and last agent error on timeout.

## Rejected

- Libguestfs package installation depends on appliance networking that is not
  available on every supported host.
- Embedded multi-step shell strings hide recipe structure and make failures
  harder to reproduce.
