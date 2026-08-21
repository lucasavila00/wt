# ADR 0053: Use a shared world creation form

- Status: Proposed
- Date: 2026-08-21

Standalone `wt new` and the Worlds activity in `wt shell` use the same Ratatui
form. It contains context, name, CPU, RAM, disk, discovered SSH-key summary,
Git author, review, and confirmation.

The form model owns values, focus, validation errors, and transitions. It does
not own terminal setup, rendering, transport, or post-create navigation.
Protocol types and validators remain the source of truth.

In `wt shell`, creation runs outside the render/input loop and reports progress
and capacity retry through Ratatui. Standalone `wt new` uses the same operation
and opens the managed SSH alias after restoring the terminal.
