# Init prompt does not explain how to install missing system packages

The disposable WT guest initialization prompt says that installing system
packages and development tooling is allowed, and that missing tooling should be
installed instead of skipping validation. It does not explain that the agent
runs as the non-root `wt` user or direct it to use `apt` through `sudo`.

This caused an avoidable failed installation attempt when Cargo was missing:

```text
$ apt-get update
E: Could not open lock file /var/lib/apt/lists/lock - open (13: Permission denied)
E: Unable to lock directory /var/lib/apt/lists/
```

The prompt should state the expected installation mechanisms explicitly, for
example:

```text
The guest runs as a non-root user. Install missing system prerequisites with
`sudo apt-get`, but install stable Rust as the normal user through rustup rather
than apt.
```

That would turn the permission to modify the disposable guest into actionable
guidance and avoid a predictable permission failure.
