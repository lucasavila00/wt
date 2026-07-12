# Config samples

Development samples only. Cargo builds do not embed or install files from this directory.

Copy a sample to any writable file outside this directory and review every
value. Pass its path to `scripts/install-server --config PATH`. The input path
has no runtime significance; the installed server reads `/etc/wt/server.toml`.

`git.identity_file` must point to an encrypted OpenSSH private key owned by the
server user with mode `0600`. Git paths may be absolute or start with `~/`. The
passphrase does not belong in this file; `wt new` prompts for it locally.
