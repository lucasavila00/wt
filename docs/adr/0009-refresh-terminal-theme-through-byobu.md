# ADR 0009: Refresh terminal theme through Byobu

- Status: Proposed
- Date: 2026-07-18

## Context

[Issue 002](../issues/002-terminal-theme-stale-through-byobu.md) reports stale
OSC 10 and OSC 11 colors after Ghostty changes theme.

Ubuntu 24.04 ships tmux 3.4. It caches the outer colors. `focus-events on`
forwards focus, but does not refresh that cache. No tmux 3.4 option does.

tmux 3.6b supports terminal light/dark reports. On a report, it queries OSC 10
and OSC 11 again before telling panes. Ghostty supports these reports.

## Decision

Build tmux 3.6b in the golden image. Pin source and checksum.

Byobu runs `/usr/bin/tmux`. Put 3.6b there. Do not rely on `PATH` or
`/usr/local/bin/tmux`.

Guest shutdown and `virt-sysprep` restore Ubuntu's tmux 3.4 binary. Preserve
the verified 3.6b binary under `/var/lib`, extract it before sysprep, then put
it back at `/usr/bin/tmux` after sysprep. Verify the final image reports 3.6b.

Cloud-init continues after a failed `runcmd` item. Run the tmux build as one
fail-fast chain. Require a separate tmux-ready marker on the host.

Keep tmux build dependencies in the guest image. Removing them made the pinned
build fail validation. tmux configure also requires `bison`. `wt-server` stays
slim.

Keep Byobu in the guest. Keep the existing OSC 52 settings. Add:

```tmux
set-option -g focus-events on
```

Do not add Codex handling. Do not use raw passthrough. Do not fake a resize.

## Verification

- Same Codex process changes dark to light and light to dark after focus returns.
- OSC 10 and OSC 11 return the new Ghostty colors.
- OSC 52 clipboard still works.

## Consequences

- Theme changes work through the normal Ghostty, tmux, and Codex protocols.
- WT owns a pinned tmux build because Ubuntu 24.04 tmux is too old.
- Ubuntu's tmux package stays installed for package policy and runtime files.
- Image preparation preserves tmux across sysprep.
- Guest image is larger because it keeps tmux build dependencies.
- Multiple attached clients with different themes remain out of scope.
