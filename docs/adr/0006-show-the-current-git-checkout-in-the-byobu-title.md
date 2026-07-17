# ADR 0006: Show the current Git checkout in the Byobu title

- Status: Proposed
- Date: 2026-07-16

## Problem statement

WT sets the terminal title for a Byobu connection from client-side world state.
It contains the qualified world name and repository name, for example
`ars.jsdev1 - frontend`.

The title does not identify the checkout currently open in the world. Worlds
with the same repository are therefore difficult to distinguish, and the title
becomes less useful after a user changes branches inside the world. The
revision requested when the world was created is not sufficient: it describes
the initial checkout, not the checkout currently on disk.

## Decision

Show the current Git checkout after the repository name:

```text
ars.jsdev1 - frontend@my-feature-branch
```

Keep the qualified world and repository name supplied by the client. In
`wt-app-shell`, read the checkout from `/workspace` before attaching to Byobu.
Use the checkout on disk as the source of truth rather than the revision stored
in the create request or instance registry.

For an attached HEAD, show the branch name returned by Git. For a detached
HEAD, show Git's abbreviated commit ID. Refresh the title on every
`ssh NAME` attach so a branch change is reflected on the next connection.

Treat title discovery as presentation, not as a prerequisite for access. If
`/workspace` is not yet a valid Git checkout, or if its repository or checkout
cannot be represented safely in a terminal title, retain the most specific
safe title available: `qualified-world - repository` or just
`qualified-world`. Setup and Byobu attachment must continue.

The title describes the root checkout at `/workspace`. It does not follow a
pane's working directory or nested repositories, and it is shared by the WT
Byobu session rather than varying by pane.

## Verification

- An attached branch renders as `qualified-world - repository@branch`.
- A detached HEAD renders with an abbreviated commit ID after `@`.
- Changing branches and reconnecting refreshes the title.
- Setup and a missing or invalid checkout retain a safe fallback title.
- Repository and checkout text cannot inject terminal or shell control
  characters.

## Consequences

- Terminal titles distinguish worlds by their live checkout.
- The displayed checkout follows changes made after world creation.
- A title can remain stale while an existing connection stays attached; the
  next attach refreshes it.
- The title represents `/workspace`, even when a pane is working in another
  repository.

## Alternatives

### Use the requested creation revision

Rejected. It becomes stale as soon as the user changes branches and does not
describe later detached checkouts.

### Update the title continuously

Rejected. Shell hooks or polling would add behavior to every pane for a value
that only needs to help users identify a connection. Refreshing on attach is
sufficient and keeps title management in the guest session entrypoint.

### Derive the checkout from each pane's working directory

Rejected. The terminal title belongs to the shared Byobu session, while panes
can have different working directories and can enter nested repositories.
