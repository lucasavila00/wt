# k8s worker

Not implemented. Target worker backend for company clusters.  
Parent: [arch README](./README.md). Control plane: [control-plane.md](./control-plane.md). Plan: [plan.md](../plan.md).

## Role

- **`wt-worker`** (or equivalent) on a **DinD-friendly** dev cluster / node pool.  
- Long-lived **Pod world** per instance; Docker-in-Docker (or equivalent) runs **stock** `.devcontainer`/compose inside.  
- Pod netns → stock ports without host port clashes.  
- CLI still talks only to the **control-plane** API (`wt-local` or later `wt-control-plane`).

```text
CLI ──► control plane ──► k8s worker ──► Pod (DinD) per name
```

## Requirements

- Cluster policy allows DinD-class workloads (often privileged), **or** another world engine (e.g. KubeVirt where available).  
- Locked-down clusters that forbid that are not a target for this backend.

## One-line summary

**Same control-plane API; worker runs compose inside pod-isolated worlds on a dev k8s pool.**
