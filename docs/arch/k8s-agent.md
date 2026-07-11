# k8s worker

Not implemented. Target worker backend for company clusters.  
Parent: [arch README](./README.md). Control plane: [control-plane.md](./control-plane.md). Plan: [plan.md](../plan.md).

## Role

- **`wt-worker`** (or equivalent) on a **DinD-friendly** dev cluster / node pool.  
- Long-lived **Pod world** per instance; Docker-in-Docker (or equivalent) runs **stock** `.devcontainer`/compose inside.  
- Pod netns → stock ports without host port clashes.  
- CLI still reaches the **control plane over SSH** (or the same SSH-first policy); not a separate public token API by default.

```text
CLI ──SSH──► control plane ──► k8s worker ──► Pod (DinD) per name
```

## Requirements

- Cluster policy allows DinD-class workloads (often privileged), **or** another world engine (e.g. KubeVirt where available).  
- Locked-down clusters that forbid that are not a target for this backend.

## One-line summary

**Same control-plane API; worker runs compose inside pod-isolated worlds on a dev k8s pool.**
