# Config samples

Development samples only. Cargo builds do not embed or install files from this directory.

Copy a sample outside this directory, review every value, and pass that path
to `scripts/install-server --config PATH`. The installer reads the input and
installs the runtime config at `/etc/wt/server.toml`. After install succeeds,
the input file is optional; reinstalls can use `/etc/wt/server.toml`.

`git.identity_file` must point to an encrypted OpenSSH private key owned by the
server user with mode `0600`. Git paths may be absolute or start with `~/`. The
passphrase does not belong in this file; `wt new` prompts for it locally.
