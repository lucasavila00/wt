# Config samples

Development samples only. Cargo builds do not embed or install files from this directory.

Copy a sample outside this directory and review every value. Pass that file to `scripts/install-server --config PATH`. The installer copies it verbatim to `/etc/wt/server.toml`.

`git.identity_file` must point to an encrypted OpenSSH private key owned by the
server user with mode `0600`. Its passphrase does not belong in this file; `wt
new` prompts for it locally.
