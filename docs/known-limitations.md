# Known limitations

## Agent gateway provider host identity

The WT agent gateway accepts the SSH host key presented by its configured Git
provider host on every connection. It does not persist or verify a provider
known-hosts file. This avoids provider host-key rotation blocking Git access.

Compromise of DNS or the network path to the provider can expose or alter Git
fetch and push traffic and can serve modified source code. Provider private
keys and API tokens remain on the WT host and are not copied into worlds.

This limitation does not apply to WT guest SSH. Managed client SSH aliases pin
each guest host key. It also does not apply to the standalone `wt-git-proxy`,
which requires a configured provider host-key pin.
