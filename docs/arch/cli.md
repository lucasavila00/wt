# Local CLI (`wt`)

Mac (or any cockpit machine) binary. No Docker. Implements the gesture in [idealized-api](../plan-reasoning/idealized-api.md).  
Parent: [arch README](./README.md). Agent it talks to (v1): [bare-metal-agent.md](./bare-metal-agent.md).

## Responsibilities

| Does | Does not |
|------|----------|
| Call agent API (create / list / destroy) | Run compose, libvirt, or clone |
| Maintain **managed** `~/.ssh/config` Host entries | Be a custom shell—enter is stock `ssh` |
| Show status / errors from agent | Own long-term instance state (agent is source of truth) |
| Point at agent base URL + auth | Know libvirt or k8s details |

## v1 commands

Illustrative; match plan gesture.

| Command | Behavior |
|---------|----------|
| `wt new <source> <name>` | `POST` create; wait or poll until `Running` or `Error`; on success write Host; print `ssh <name>` |
| `wt ls` | `GET` list; table name / status / ssh target |
| `wt rm <name>` | `DELETE`; remove Host entry |
| `wt config` / flags | Agent URL, token, ssh config path (defaults sane) |

No `wt sh` required if Host + `RemoteCommand`/sshd setup is enough—optional sugar later.

## SSH config

- Managed block or `Include` file (e.g. `~/.config/wt/ssh_config`) included from `~/.ssh/config`.  
- On success of `new`:

```text
Host <name>
  HostName <guest-ip-or-dns>
  User <world-user>
  IdentityFile <key>
  # optional: StrictHostKeyChecking / known_hosts path under wt state
```

- On `rm`: delete that Host.  
- Do not hand-edit the managed section.

v1 can skip fancy `RemoteCommand` (byobu); land on guest shell first, improve landing later.

## Config / state on Mac

| Item | Where (sketch) |
|------|----------------|
| Agent URL, token | `~/.config/wt/config.toml` |
| SSH include | `~/.config/wt/ssh_config` |
| Optional cache of last list | optional; **agent wins** on conflict |

## Language / code layout

**Rust** binary in the same workspace as the agent ([README](./README.md)). Depends on `wt-api` for types. HTTP client (`reqwest` or similar).

Cross-compile to Mac from CI or build on Mac—fine for v1 single dev.

## Failure UX

- Agent unreachable → clear error, no half-written Host (or roll back Host write).  
- Create fails mid-provision → surface agent error; `ls` shows `Error`; `rm` still cleans.  
- Do not leave stale Host pointing at dead IP without saying so (`ls` / probe later).

## Out of scope for CLI (v1)

- Provider-specific flags (`--libvirt-pool`, kubecontext) beyond choosing agent URL  
- Browser tunnels  
- Implementing a second protocol for k8s—same HTTP API when that agent exists  

## One-line summary

**Thin Rust CLI: talk to agent, write `Host <name>`, get out of the way for `ssh`.**
