# Terminal theme changes are stale through WT Byobu

## Bug

Terminal applications inside a WT Byobu session do not follow a light or dark
theme change made by the workstation terminal.

The observed control case works:

1. Run Codex CLI directly in Ghostty on macOS.
2. Change the macOS appearance between light and dark.
3. Ghostty updates and Codex updates its colors.

The same Codex version in a current WT world does not update after Ghostty
changes. The affected path is:

```text
Codex -> app SSH -> WT guest Byobu/tmux -> workstation SSH -> Ghostty
```

This issue concerns a single attached workstation client. Multiple tmux clients
with different terminal themes are inherently ambiguous and are outside the
initial fix.

## Evidence

Codex queries the terminal's default foreground and background with OSC 10 and
OSC 11. It caches both values and queries them again after a terminal focus-gain
event. Its Unix startup probe uses a 100 ms timeout.

Codex sends raw queries equivalent to:

```text
ESC ] 10 ; ? ESC \
ESC ] 11 ; ? ESC \
```

tmux 3.4 does not simply forward those raw queries. It parses them and replies
using foreground and background values cached on the attached tmux client.
tmux obtains those values by issuing its own OSC 10 and OSC 11 queries to the
outer terminal. It sends the initial requests when the client attaches and can
repeat them on selected client events, including resize, with a 30-second rate
limit.

WT currently creates `/usr/local/share/wt-tmux.conf` with clipboard handling
and passthrough enabled:

```tmux
set-option -s set-clipboard on
set-option -g allow-passthrough on
set-option -as terminal-features ',xterm-ghostty:clipboard'
```

That configuration fixed [Issue 001](./001-osc52-clipboard-blocked-by-byobu.md),
but it does not make raw OSC 10 and OSC 11 bypass tmux. `allow-passthrough`
applies only when an application explicitly wraps a sequence in tmux's DCS
passthrough envelope.

WT also does not enable tmux's `focus-events` option. Codex requests terminal
focus reporting, but tmux does not forward outer focus changes into panes when
that option is off. Even with focus forwarding enabled, Codex can still receive
stale colors if tmux has not refreshed its own cached client colors first.

Relevant upstream implementations, inspected on 2026-07-18:

- [Codex terminal palette cache and focus re-query](https://github.com/openai/codex/blob/main/codex-rs/tui/src/terminal_palette.rs)
- [Codex focus-gain handling](https://github.com/openai/codex/blob/main/codex-rs/tui/src/tui/event_stream.rs)
- [Codex OSC 10/11 startup probe](https://github.com/openai/codex/blob/main/codex-rs/tui/src/terminal_probe.rs)
- [tmux 3.4 OSC 10/11 pane replies](https://github.com/tmux/tmux/blob/3.4/input.c)
- [tmux 3.4 outer-terminal color queries](https://github.com/tmux/tmux/blob/3.4/tty.c)

## Expected

With one Ghostty client attached through `ssh NAME`, an application in the WT
devcontainer that supports OSC 10/11 and focus events receives the current
Ghostty foreground and background after macOS appearance changes. A running
Codex process should update just as it does outside WT, without restarting the
world, tmux server, pane, or Codex.

## Investigation

Determine which part of the update is missing in the provisioned Ubuntu 24.04
tmux 3.4 environment:

1. Confirm the outer Ghostty client reports new OSC 10/11 values after the
   macOS appearance change.
2. Confirm tmux's cached client foreground and background update while the
   existing client remains attached.
3. Enable `focus-events` experimentally and confirm Codex receives focus-lost
   and focus-gained events through the guest tmux and app SSH layers.
4. After focus gain, confirm Codex's OSC 10/11 re-query receives the new values
   rather than tmux's previous cached values.
5. Check whether detach and reattach, a terminal resize after 30 seconds, or a
   new Codex process changes the result. These distinguish stale tmux client
   state from missing pane focus forwarding.

Useful configuration checks in the guest are:

```sh
/usr/bin/byobu-tmux show-options -g focus-events
/usr/bin/byobu-tmux show-options -g allow-passthrough
```

## Candidate fix

First test adding the following WT-owned tmux setting:

```tmux
set-option -g focus-events on
```

This is necessary for focus-aware applications such as Codex to observe outer
terminal focus changes. Do not accept it as the complete fix unless tmux also
refreshes its cached client colors before answering the pane's subsequent OSC
10/11 queries.

If tmux 3.4 keeps stale colors, evaluate the smallest reliable way to trigger
its existing outer-terminal color re-query when the client regains focus. If no
tmux command or hook provides that behavior, the remaining fix belongs upstream
in tmux or in a newer tmux release and WT should document the limitation rather
than add application-specific Codex handling.

Do not replace the current `TERM` value, enable unrestricted passthrough, or
hard-code a light or dark theme as a fix. Those approaches either do not refresh
OSC 10/11 state, weaken the terminal boundary, or discard automatic theme
selection.

## Regression test

Using one Ghostty window attached through `ssh NAME`:

1. Start Codex in the devcontainer with automatic theme selection.
2. Switch macOS from dark to light while keeping the same WT tmux client, pane,
   and Codex process.
3. Return focus to Ghostty and verify Codex renders with its light palette.
4. Switch macOS from light to dark and verify the same process renders with its
   dark palette.
5. Repeat the control case with Codex directly in Ghostty.

Also verify that OSC 52 clipboard behavior from
[ADR 0008](../adr/0008-allow-osc52-clipboard-through-byobu.md) still works. The
theme fix must not broaden passthrough from invisible panes or regress the
existing clipboard path.

