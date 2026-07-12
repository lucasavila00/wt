# wt-command

Shared builder for local process commands.

`cmd!` accepts heterogeneous argument expressions and returns an owned
`std::process::Command`. It does not execute the command.

```rust
use wt_command::cmd;

let command = cmd!("install", "-m", format!("{:04o}", 0o640), destination);
```
