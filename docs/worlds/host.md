# Host worlds

A host world is a retained Ubuntu guest created from the server's verified
golden image. `wt new` asks for its name and resources, creates it, updates the
managed SSH inventory, and opens its persistent Byobu workspace.

The golden image contains OpenSSH, Byobu, tmux, Codex, Diffo, and WT's Git and
agent tool helpers. World creation only applies identity and access state; it
does not execute a user recipe or install application packages.

Managed SSH provides two aliases:

- `CONTEXT.WORLD` opens the persistent Byobu workspace.
- `CONTEXT.WORLD-direct` opens a normal guest shell or runs a command directly.

World disks are writable overlays backed by a retained golden-image generation.
Stop/start preserves their data. Deleting a world revokes its Git grant and
removes its overlay.
