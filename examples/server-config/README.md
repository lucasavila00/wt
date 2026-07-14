# Server config

The sample is an install input for:

```text
scripts/install-server --config PATH
```

Copy it outside this directory and review every value. Setup writes the strict
runtime config to `/etc/wt/server.toml`. Keep the input for reinstalling the same
configuration.

`git.known_hosts_file` pins Git host keys. Clone authentication comes from the
workstation SSH agent forwarded during the first world connection. Paths may be
absolute or start with `~/`. Changing strict server settings requires clearing
server state and reinstalling.
