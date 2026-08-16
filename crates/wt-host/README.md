# wt-host

Raw Ubuntu host world lifecycle.

It prepares the `wt` login through QGA, stages the operator YAML, pins the guest
SSH identity, installs the agent Git relay and clients, proves login with a
one-use key, and reports first-SSH cloud-init setup state.

Machine lifecycle stays behind `wt-provider`; SSH aliases stay in `wt-cli`.

Contract: [Host worlds](../../docs/worlds/host.md).
