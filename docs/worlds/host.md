# Host worlds

A host world is a retained Ubuntu guest configured by cloud-init. It has no
implicit checkout, Git grant, Docker setup, devcontainer, or app SSH server. A
recipe may create its own checkout, as the project example does.

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
store.

Success changes the world from `setup` to `running`. Failure changes it to
`error` and keeps both SSH aliases. Inspect it with `NAME-vs`, then run
`wt rm NAME` and recreate it. WT never reruns a failed recipe.

The regular alias attaches to a persistent Byobu session. The `-vs` alias is
the same guest SSH endpoint with no forced command. Both forward the
workstation's SSH agent:

```text
ssh CONTEXT.NAME
ssh CONTEXT.NAME-vs
```

There is no `-host` alias. `wt code` rejects host worlds; use `-vs` directly for
plain SSH, SFTP, or an editor.

Byobu uses a stable agent socket that is refreshed on every connection. Keys
are not copied into the world, but any process controlling this trusted host
can use the agent while connected. Keep the setup connection open while the
recipe needs it.

The host image is separate from the devcontainer image. It adds OpenSSH, QEMU
guest support, the pinned Byobu package, compiled tmux, Ghostty terminfo, and
the shared WT terminal profile. Ubuntu's Git remains available, but WT adds no
checkout, Git grant, agent socket, or provider credentials.

Use the checked-in
[host-world recipe](../../examples/host-world/cloud-init.yaml) for a complete
WT development environment with Rust, Codex, and a public checkout. It verifies
agent forwarding without using real credentials and cannot run the real KVM E2E
from inside the world.
