# ADR 0038: Make `wt shell` terminal-theme safe

- Status: Accepted
- Date: 2026-08-22

## Problem

`wt shell` uses ANSI black for its background and dark gray for chrome. Both
are palette slots, not fixed colours. Ghostty changes the active palette, so
black can be light gray in a light theme. The menu then loses contrast.

## Decision

Render the menu's background and ordinary text with the terminal defaults.
Represent selection by reversing those defaults, rather than by supplying a
dark background. Status labels must retain text meaning; colour only supports it.

This avoids guessing whether the terminal is light or dark, querying its
palette, or maintaining a WT theme. A running menu follows a terminal theme
change because the terminal owns its default foreground and background.

## Constraints

Do not use ANSI black or dark gray for structural surfaces, essential text, or
selection. Any remaining accent must be readable on both default backgrounds.

## Verification

Snapshot the intended default and reversed styles. Test the control menu in
Ghostty light and dark themes, including changing theme while it is open.
