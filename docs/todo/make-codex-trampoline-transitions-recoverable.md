# Make Codex trampoline transitions recoverable

Codex trampoline installation first moves the real CLI away and only then
publishes the trampoline. A process kill or power loss between those operations
leaves no `codex` command at its expected path. Uninstallation has the inverse
window because it removes the trampoline before restoring the real CLI.

Stage the shim before changing the live path and use an atomic exchange where
available. Define recovery for every recognized interrupted state so the next
provisioning attempt restores an executable `codex` command. Add failpoint tests
around each transition step and verify recovery from every intermediate state.

Relevant code: `crates/products/wt/codex-integration/src/install.rs`.
