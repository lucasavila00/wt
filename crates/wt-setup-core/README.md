# wt-setup-core

Shared host setup utilities for WT installers.

It provides command execution, privileged file installation, ownership and
mode validation, home-directory expansion, and SSH credential validation and
preparation. Encrypted private keys are unlocked through an injected
passphrase prompt and staged in private temporary files.

The crate contains no WT server or Git proxy installation policy. That stays
in `wt-server-setup` and `wt-git-proxy-setup`.
