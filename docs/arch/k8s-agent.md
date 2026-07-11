# k8s agent (deferred)

**Not iteration 1.** Exists so the architecture slot is clear. Do not design or build this until bare-metal path is stable for real use.  
Parent: [arch README](./README.md). Plan intent: [plan.md](../plan.md).

## Intended role (later)

Same **agent API** as bare-metal (`wt-api` types). Create **long-lived Pod worlds** with Docker-in-Docker (or equivalent) so **stock** `.devcontainer`/compose runs inside; pod netns gives port isolation on shared nodes.

```text
CLI ── same HTTP API ──► k8s agent ──► Pod (DinD) world per name
```

Requires a **DinD-friendly** dev cluster/node pool—not every prod cluster.

## Why deferred

- First prove CLI + SSH Host + recipe on **libvirt** ([bare-metal-agent.md](./bare-metal-agent.md)).  
- k8s adds policy, privileges, and cluster variance; wrong place to debug product UX.  
- Single language/`wt-api` means this agent is “another binary implementing the same crate,” not a rewrite.

## Do not decide yet

- DinD image, privileged vs rootless  
- SSH into pod vs `ProxyCommand`/kubectl  
- Multi-cluster selection UX  

## One-line summary

**Same API, later backend—ignore until bare metal is boringly solid.**
