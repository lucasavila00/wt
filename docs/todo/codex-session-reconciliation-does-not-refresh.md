# Make Codex session reconciliation reliable and observable

Shared rollout files can be present in a world while the host Codex session
picker remains empty. `wt-codex-integration` is intended to refresh Codex's
local session index before starting the real Codex process, but that refresh is
not currently making the shared sessions visible.

Fix the startup reconciliation and verify that it completes before Codex is
executed. If reconciliation fails, keep Codex fail-open but record the complete
failure somewhere durable and inspectable; the current startup path can lose
its stderr diagnostic and leave no evidence of why synchronization failed.
