# The devcontainer issue

Why default devcontainers fight **N parallel instances**, and how this project handles it.  
Plan: [../plan.md](../plan.md). Isolation: [isolation-without-port-overrides.md](./isolation-without-port-overrides.md).

## Default model (“clone mirror”)

1. One git checkout on the host  
2. Bind-mount into the container  
3. Tools see `/workspaces/...`  
4. Naming often derives from that single path  

**Good for:** one feature at a time.  
**Bad for:** N parallel stacks from one tree without careful naming.

**CI** avoids this with a fresh clone per job. Devcontainer UX is single-workspace-oriented.

## Project approach

```text
instance name → remote world → one clone inside that world → stock recipe
```

Classic single-workspace devcontainer is fine **inside** one world. Pain returns only if many instances share one host tree + one Docker engine.

## Recipe reuse

| Path | Status |
|------|--------|
| **Reuse `.devcontainer` + Compose** | **Canonical** for `wt` |
| Image + Compose only at runtime | Acceptable if driven from the same config |
| New env format for the tool | **Out of scope** |
| Outsourced SaaS runner as the product | Out of scope |

GitLab CI remains a separate batch file; shared value is **images**, not inventing a third dialect.

## Worktrees

Not the product identity. Prefer **clone per world**. Worktrees need explicit gitdir mounts and UID policy if ever used.

## Spec note

[containers.dev](https://containers.dev/) is a **recipe** format (multi-service via Compose), not a multi-**instance** fleet protocol. Multiplicity is **worlds + control plane**, not a new chapter of the spec.

## Lean

- Keep the existing recipe  
- One world + one clone per instance  
- Compose runs **inside** the KVM world, not as a rewrite to another deployment format

## One-line summary

**Same devcontainer/compose; separate remote worlds and clones do multiplicity.**
