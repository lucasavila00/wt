# Copy Git author identity into new worlds

## Problem

WT copies Git authentication into a new world. It does not copy Git author
identity.

No component sets:

- `user.name`
- `user.email`

The first commit can fail with `Author identity unknown`.

The SSH key cannot provide these values. It authenticates Git access. It does
not identify the commit author.

## Proposed behavior

On `wt new`, read the workstation's global values:

```text
git config --global --get user.name
git config --global --get user.email
```

Send both values in the create request.

Set them in the new checkout:

```text
git -C /workspace config --local user.name "$name"
git -C /workspace config --local user.email "$email"
```

Use repository-local config. The checkout is mounted into the devcontainer, so
the values work in both the guest and the devcontainer.

Do not copy the workstation's Git config file. It can contain credential
helpers, aliases, conditional includes, and workstation-only paths.

## Missing values

Do not fail world creation.

Copy each value that exists. Warn about each missing value:

```text
Git user.email was not copied because it is not configured on this workstation.
Set it with `git config --global user.email ADDRESS` or configure it in the world.
```

## Scope

Copy only `user.name` and `user.email`.

Do not copy signing settings, credential helpers, aliases, or other Git config.

Keep the create API fields optional for compatibility with older clients.

Possible later work: add `wt new` flags for repository-specific identities.
Conditional includes need separate design because the workstation path does not
match `/workspace`.

## Tests

Cover:

- both values present;
- one value missing;
- both values missing;
- spaces and non-ASCII characters; and
- a commit from the primary devcontainer with the copied author.
