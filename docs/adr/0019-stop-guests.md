# ADR 0019: Stop guests

- Status: Accepted
- Date: 2026-08-20

`wt stop NAME` asks the guest to shut down cleanly and waits for libvirt. A
stopped world keeps its independent disk, machine definition, metadata, SSH
identity, Git grant, and server-backed Codex data, but releases CPU and memory.

`wt start` atomically reacquires CPU, memory, and disk capacity before booting
the existing disk. WT does not preserve RAM or live processes and does not
infer task completion from an SSH disconnect or agent exit.
