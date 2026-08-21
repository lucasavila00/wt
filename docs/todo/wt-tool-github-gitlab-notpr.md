# Make the wt-tools provider explicit

`wt-tools` currently runs `git config --get remote.origin.url` in the current
working directory and sends the parsed host and project to the gateway. The
provider is therefore selected implicitly by whichever Git checkout contains
the process. Running the same command from a different directory can target a
different provider or fail because no origin is available.

Provider selection should be visible in the command itself. Prefer a
provider-first interface such as:

```text
wt-tools github mr open ...
wt-tools github mr show ...
wt-tools gitlab mr open ...
wt-tools gitlab mr show ...
```

The gateway must still enforce the authenticated world's grant and repository
scope. Making the provider explicit is about a predictable command contract,
not expanding access.

Relevant code:

- `crates/products/agent-tools/gateway/src/bin/wt-tools.rs`
- `crates/products/agent-tools/gateway/src/gateway/service.rs`
