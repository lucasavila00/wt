# ADR 0009: Refresh terminal theme through Byobu

- Status: Accepted
- Date: 2026-07-18
- Amended by: [ADR 0027](0027-build-images-in-kvm.md)

## Context

OSC 10 and OSC 11 colors stayed stale after Ghostty changed theme through WT's
Byobu session.

Ubuntu 24.04 ships tmux 3.4. It caches the outer colors. `focus-events on`
forwards focus, but does not refresh that cache. No tmux 3.4 option does.

tmux 3.6b supports terminal light/dark reports. On a report, it queries OSC 10
and OSC 11 again before telling panes. Ghostty supports these reports.

## Decision

Build tmux 3.6b through the shared recipe for both world images. Pin source and
checksum.

Byobu runs `/usr/bin/tmux`. Put 3.6b there. Do not rely on `PATH` or
`/usr/local/bin/tmux`.

Guest shutdown and `virt-sysprep` restore Ubuntu's tmux 3.4 binary. Preserve
the verified 3.6b binary under `/var/lib`, extract it before sysprep, then put
it back at `/usr/bin/tmux` after sysprep. Verify the final image reports 3.6b.

The shared image builder writes its ready marker only after tmux and the terminal
assets pass validation.

Install tmux build dependencies only for the KVM image build. Remove them after
the pinned binary and terminal assets pass validation.

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
- Build dependencies do not remain in the installed image.
- Multiple attached clients with different themes remain out of scope.
