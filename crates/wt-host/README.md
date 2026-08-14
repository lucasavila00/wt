# wt-host

Raw Ubuntu host world lifecycle.

It renders WT-owned NoCloud vendor-data, passes operator user-data through,
waits for cloud-init, pins the guest SSH identity, proves `wt` login with a
one-use key, and removes that key.

Machine lifecycle stays behind `wt-provider`; SSH aliases stay in `wt-cli`.

Contract: [Host worlds](../../docs/worlds/host.md).
