# ADR 0053: Use a shared world creation form

- Status: Proposed; Date: 2026-08-21

## Decision

Replace the Cliclack `wt new` prompts with one Ratatui form shared by standalone
`wt new` and the Worlds activity in `wt shell`.

Keep the current command meanings. `wt new` and `wt new host` open a host form;
`wt new dev` opens a development form. The `wt shell` command palette opens the
same form with its kind already selected. It does not spawn a nested `wt new`
process.

The form contains:

- context, name, CPU, RAM, disk, and discovered SSH-key summary;
- cloud-init user-data path for a host;
- Git repository and base branch for a development world;
- a review and confirmation step.

The form model owns field values, focus, validation errors, and transitions. It
does not own terminal setup, rendering, transport calls, or post-create
navigation. Protocol types and validators remain the source of truth; the UI
does not duplicate creation rules.

`Up`, `Down`, `Tab`, and `Shift-Tab` move focus. `Left` and `Right` change a
selector, and `Enter` advances or confirms. `Escape` cancels. While the form is
active it owns these keys before activity navigation. `F5` only toggles the
shell control layer and does not cancel the form; reopening it restores the form
state. `F6` exits.

Confirmation produces the existing typed `CreateInstance` request. In
`wt shell`, creation runs outside the render/input loop and reports progress and
capacity retry through Ratatui so live world terminals keep draining. Standalone
`wt new` uses the same operation and preserves its existing SSH handoff after
the terminal is restored.

The reusable boundary is the form state and creation operation, not a general UI
framework. Other client commands are unchanged.

## Consequences

- Standalone and shell creation cannot drift in fields or validation.
- `wt shell` avoids nested raw-terminal ownership and blocking session output.
- The current Cliclack creation prompts and spinner are removed when the form
  ships.
- Server APIs, creation semantics, and ADRs 0004 and 0011 remain unchanged.
