# ADR 0086: Separate agent execution from WT

WT owns worlds and a generic bounded command transport. It validates world ownership,
executes an absolute executable with argv/stdin as the guest user, and returns UTF-8
stdout, stderr, and exit status. It does not interpret or retry agent operations.

agapi is a separate product in this repository, with its own binary, version,
installer, release workflow, supported Codex version, and state directory.
Neither wtg nor wts links its implementation. Agent updates happen inside existing
environments without a WT release or image rebuild.

agapi initially adapts Codex App Server. It validates the installed version, owns
thread/turn IDs and provider-specific recovery, and persists mutation receipts and
terminal results. Uncertain submissions never replay automatically. Resume does not
submit a prompt. Consumers import cursor-addressed events before acknowledging.

WT and agapi each expose a JSON client with an injected transport. Controllers
compose agapi over WT execution or run the same agapi executable locally.
Local execution is not an isolation boundary. No container provider is implemented.

Real Codex compatibility tests run against a localhost model endpoint.
WT's former agent-specific operations, compiled wrapper, image-installed Codex,
and completion services are removed. Existing authentication/history mounts remain
world infrastructure; they do not control the agent runtime lifecycle.
