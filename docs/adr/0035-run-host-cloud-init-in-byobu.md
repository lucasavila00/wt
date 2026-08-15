# ADR 0035: Run host cloud-init in Byobu

- Status: Accepted
- Date: 2026-08-15
- Amends: [ADR 0026](0026-make-world-kinds-first-class.md),
  [ADR 0033](0033-forward-ssh-agents-to-host-worlds.md), and
  [ADR 0034](0034-retain-failed-host-worlds.md)

## Context

WT currently runs the submitted cloud-init recipe before host SSH is available.
The user cannot see the work in Byobu, and the recipe cannot use the SSH agent
forwarded from the workstation. We already fixed this lifecycle mistake for
devcontainer setup.

## Decision

Host creation has two stages.

First, WT boots Ubuntu, creates the `wt` login, stages the submitted YAML, and
verifies SSH. The world enters `setup`. `wt new host` then opens its regular
Byobu alias with agent forwarding.

The first Byobu session starts cloud-init's config and final stages with the
submitted YAML. Its output stays in the pane and the normal cloud-init log. The
recipe uses the stable forwarded-agent socket from ADR 0033.

The boot seed contains only WT's minimal network configuration. The submitted
YAML is stored root-only in the guest and passed to cloud-init unchanged. It is
not stored in SQLite.

A completion marker promotes the world to `running`. A failure marker moves it
to `error`. Failed hosts keep both SSH aliases so they can be inspected and
removed. WT never reruns a failed recipe because cloud-init commands may not be
safe to repeat.

The direct `-vs` alias does not start setup. If the Byobu connection closes,
cloud-init keeps running. Work that still needs the forwarded agent may fail;
reconnecting refreshes the socket but does not retry failed commands.

The API remains protocol v1. Host progress no longer needs a second stream on
the control socket: Byobu and `/var/log/cloud-init-output.log` own it.

Host and devcontainer worlds share the same machine, SSH, Byobu, registry, and
lifecycle code. Their setup scripts stay separate because they run different
applications.
