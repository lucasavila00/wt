# Server config

The sample is an install input for:

```text
scripts/install-server --config PATH
```

Copy it outside this directory and review every value. Setup writes the strict
runtime config to `/etc/wt/server.toml`. Keep the input for reinstalling the same
configuration.

Each `agent_git` provider names an API-token file, SSH key pair, and trusted
host-key file. Paths may be absolute or start with `~/`. The installer validates
them and stores encrypted copies for the gateway; worlds never receive them.
Changing strict server settings requires `make nuke` followed by reinstalling.
