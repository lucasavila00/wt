# Server config

The sample is an install input for:

```text
scripts/install-server --config PATH
```

Copy it outside this directory and review every value. Setup writes the strict
runtime config to `/etc/wt/server.toml`. Keep the input for reinstalling the same
configuration.

`git.identity_file` must be an encrypted OpenSSH private key owned by the server
user with mode `0600`. Paths may be absolute or start with `~/`.

`guest.session` must be `tmux` or `byobu`. Changing strict server settings
requires clearing server state and reinstalling.
