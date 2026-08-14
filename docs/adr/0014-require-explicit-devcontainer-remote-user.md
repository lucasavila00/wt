# ADR 0014: Require an explicit devcontainer remote user

- Status: Accepted
- Date: 2026-07-31

## Context

WT needs one devcontainer user for SSH, Byobu, VS Code, Git configuration, and
authorized keys.

Today WT looks for `remoteUser`, then `containerUser`, then falls back to
`root`. This can silently choose the wrong user. In one real case, the checkout
belonged to `vscode`, but WT connected as `root`. Git reported dubious
ownership, and adding `safe.directory` hid the actual problem.

WT cannot guess which user the repository author intended.

## Decision

Require `remoteUser` in the repository's devcontainer configuration. Do not
fall back to `containerUser`, Dockerfile `USER`, feature detection, or `root`.

Before starting the devcontainer, WT reads the resolved configuration and
validates `remoteUser` in Rust. After startup, WT verifies that the account
exists in the container and that runtime metadata reports the same user.

When recovering a stopped world, WT again reads and validates the live runtime
metadata before restoring the user's app SSH authorized keys. It does not trust
a separately persisted username. See
[ADR 0025](0025-recover-world-containers-after-guest-start.md).

Missing, invalid, nonexistent, or conflicting users fail setup with a direct
error. WT does not install keys or mark the world complete.

An explicit `remoteUser: "root"` is allowed. Implicit root is not.

## Verification

- Missing or invalid `remoteUser` fails before container startup.
- A user that does not exist fails before setup completes.
- Runtime metadata cannot replace the configured user.
- Restart recovery accepts only the validated live runtime user.
- SSH, Byobu, VS Code, and Git configuration use the declared user.
- Explicit root still works.

## Consequences

Repositories that relied on user inference must add `remoteUser`.

WT gets one clear user contract and no longer turns incomplete configuration
into a root shell. `safe.directory` remains available for real bind-mount
ownership differences, but not as a workaround for choosing the wrong user.

## Alternatives

Keep the current fallbacks. Rejected because each fallback adds another way to
silently select an unintended user. A setup error is easier to understand and
fix.
