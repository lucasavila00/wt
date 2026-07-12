# wt-command

Shared local process command builder for WT crates.

The `cmd!` macro builds an owned [`std::process::Command`] from a program and
heterogeneous argument expressions. It only constructs the command; callers
remain responsible for configuring and executing it.

```rust
use wt_command::cmd;

let command = cmd!("install", "-m", format!("{:04o}", 0o640), destination);
```

Used by the CLI, guest helpers, libvirt backend, server setup, and integration
tests.
