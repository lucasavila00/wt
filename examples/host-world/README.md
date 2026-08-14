# Host world

`cloud-init.yaml` is a complete host-world recipe. It writes a root-owned
configuration file, installs a small command, and records that `runcmd`
finished.

```text
wt new host examples/host-world/cloud-init.yaml
ssh CONTEXT.NAME
ssh CONTEXT.NAME-vs /usr/local/bin/wt-host-example
ssh CONTEXT.NAME-vs sudo -n test -f /var/lib/wt-host-example-ready
```

The regular alias attaches to Byobu. Use `-vs` for direct SSH and remote
commands.
