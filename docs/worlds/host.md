# Host worlds

A host world is a retained Ubuntu guest configured by cloud-init. It has no
implicit checkout, Docker setup, devcontainer, or app SSH server. A recipe may
create its own checkout, as the project example does.

Create one with a non-empty cloud-init user-data file:

```text
wt new host ./host.yaml
```

WT first boots the guest, creates `wt`, stages the exact file root-only, and
verifies SSH. `wt new host` then opens Byobu. Cloud-init starts there with the
workstation SSH agent and runs the standard init, config, and final stages.
Output stays in the pane and `/var/log/cloud-init-output.log`.

The recipe is included in a hashed create fingerprint but is not stored in
SQLite. It and its output remain on the guest disk, so neither is a secret
store. WT rejects top-level host-key, cloud-init stage, merge, and output fields
because it owns those parts of setup.

Success changes the world from `setup` to `running`. A setup failure changes it
to `error` and keeps both SSH aliases. Earlier provisioning failures have no SSH
alias. Every failed host remains visible in `wt ls` and removable with `wt rm`.
WT never reruns a failed recipe.

The regular alias attaches to a persistent Byobu session. The `-vs` alias is
the same guest SSH endpoint with no forced command. Both forward the
workstation's SSH agent:

```text
ssh CONTEXT.NAME
ssh CONTEXT.NAME-vs
```

`wt ssh NAME` refreshes the managed aliases and connects to the qualified
persistent Byobu alias.

There is no `-host` alias. `wt code` rejects host worlds; use `-vs` directly for
plain SSH, SFTP, or an editor.

Byobu uses a stable agent socket that is refreshed on every connection. Keys
are not copied into the world, but any process controlling this trusted host
can use the agent while connected. Keep the setup connection open while the
recipe needs it.

Every host receives `ag-git` and a revocable gateway grant. Configured provider
URLs use the gateway automatically. The grant can read every available
repository and write only branches under `wt/`; provider credentials remain on
the server. This branch restriction applies to gateway traffic. The forwarded
workstation agent is a separate access path and is not restricted by the
gateway.

The host image is separate from the devcontainer image. It adds OpenSSH, QEMU
guest support, the pinned Byobu package, compiled tmux, Ghostty terminfo, and
the shared WT terminal profile. Ubuntu's Git remains available, and WT adds no
implicit checkout or provider credentials.

Use the checked-in
[host-world recipe](../../examples/host-world/cloud-init.yaml) for a complete
WT development environment with Rust, Codex, and a public checkout. It verifies
agent forwarding without using real credentials and cannot run the real KVM E2E
from inside the world.
