# ADR 0016: Remove redundant world boot work

- Status: Accepted
- Date: 2026-08-01

## Context

WT builds and validates a golden image before it creates worlds. The image
manifest requires `qemu-guest-agent`, and image construction enables its
service.

The per-world NoCloud seed also updated package indexes, installed
`qemu-guest-agent`, and enabled the service. This gave the golden-image build
and each machine boot overlapping responsibility for the same prerequisite.
It also allowed cloud-init and world setup to run separate package-manager
transactions during the same boot.

The machine lifecycle discovers guest-agent and DHCP readiness by polling.
Those checks observe state; they do not own guest configuration.

## Decision

Make the validated golden image the only owner of machine prerequisites.
`wt-server-setup` installs `qemu-guest-agent`, enables its service, records its
version in the image manifest, and rejects image drift.

Use the per-world NoCloud seed only for instance identity and network
configuration. Machine boot does not update packages, install the guest agent,
or repair golden-image package drift.

Poll guest-agent readiness every second and DHCP readiness every 250
milliseconds instead of polling both every two seconds.

## Verification

- Image-policy tests require `qemu-guest-agent` in the golden-image manifest.
- Image construction enables the guest-agent service.
- The generated per-world cloud config contains no package or service actions.
- Real KVM creation reaches the guest agent and DHCP using the validated image.

## Consequences

Machine dependencies have one owner and one validation boundary. Changing a
machine prerequisite requires rebuilding the golden image through
`wt-server-setup`.

The libvirt provider can rely on the validated image instead of mutating the
guest during every boot. Images without the required manifest or package remain
invalid WT images.

Readiness checks respond more closely to actual guest state without changing
their timeout or failure behavior.

## Alternatives

Keep package installation in both the golden-image build and per-world
cloud-init. Rejected because it creates two owners for the same machine
prerequisite and permits competing package-manager transactions during boot.
