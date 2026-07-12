# Config samples

Install-input samples for `scripts/install-server --config PATH`.

Copy a sample outside this directory, review every value, and pass that path to
the installer. Setup materializes `/etc/wt/server.toml` from it. Keep the file
for a later clear + reinstall if you want the same settings.

`git.identity_file` must point to an encrypted OpenSSH private key owned by the
server user with mode `0600`. Git paths may be absolute or start with `~/`.
`wt new` prompts for the passphrase.
