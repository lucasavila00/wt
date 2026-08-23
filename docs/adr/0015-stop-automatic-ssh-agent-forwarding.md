# ADR 0015: Stop automatic SSH-agent forwarding

- Status: Accepted
- Date: 2026-08-19

Managed world aliases do not enable SSH-agent forwarding. Provider Git access
uses scoped gateway grants, so guest code does not need the workstation agent.
A user may explicitly opt into native forwarding for one direct connection
with `ssh -A CONTEXT.WORLD-direct`; that bypasses gateway policy and is outside
WT's managed contract.
