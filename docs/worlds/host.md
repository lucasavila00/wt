# Host worlds

A host world is a retained Ubuntu guest configured by cloud-init. It has no
implicit checkout, Git grant, Docker setup, devcontainer, or app SSH server. A
recipe may create its own checkout, as the project example does.

Create one with a non-empty cloud-init user-data file:

```text
wt new host ./host.yaml
```

WT passes the file through unchanged. A separate NoCloud vendor document owns
the fixed `wt` login, sudo access, and the public keys selected by the client.
User-data runs as root and can override that state.

Creation succeeds only after cloud-init completes, the guest SSH host identity
matches, and a one-use key can log in as `wt`. WT removes that key before
returning. The recipe is included in a hashed create fingerprint but is not
stored in SQLite. Plaintext user-data remains in the machine's NoCloud files
and inside the guest, so it is not a secret store.

The regular alias attaches to a persistent Byobu session. The `-vs` alias is
the same guest SSH endpoint with no forced command. Both forward the
workstation's SSH agent:

```text
ssh CONTEXT.NAME
ssh CONTEXT.NAME-vs
```

There is no `-host` alias. `wt code` rejects host worlds; use `-vs` directly for
plain SSH, SFTP, or an editor.

Byobu uses a stable agent socket that is refreshed on every connection, so Git
keeps working after reconnects. Keys are not copied into the world, but any
process controlling this trusted host can use the agent while connected.

The host image is separate from the devcontainer image. It adds OpenSSH, QEMU
guest support, the pinned Byobu package, compiled tmux, Ghostty terminfo, and
the shared WT terminal profile. Ubuntu's Git remains available, but WT adds no
checkout, Git grant, agent socket, or provider credentials.

Use the checked-in
[host-world recipe](../../examples/host-world/cloud-init.yaml) for a complete
WT development environment with Rust, Codex, and a public checkout. It contains
no credentials and cannot run the real KVM E2E from inside the world.
