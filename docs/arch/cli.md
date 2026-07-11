# Local CLI (`wt`)

Mac (or any cockpit machine) binary. No Docker. Implements the gesture in [idealized-api](../plan-reasoning/idealized-api.md).  
Parent: [arch README](./README.md). Agent it talks to (v1): [bare-metal-agent.md](./bare-metal-agent.md).

## Responsibilities

| Does | Does not |
|------|----------|
| Call agent API (create / list / destroy) | Run compose, libvirt, or clone |
| **Print** (early) or later **apply** SSH `Host` snippets | Be a custom shell—enter is stock `ssh` |
| Show status / errors from agent | Own long-term instance state (agent is source of truth) |
| Point at agent base URL + auth | Know libvirt or k8s details |

**SSH config:** v1 eras print the delta only; auto-edit of `~/.ssh/config` / managed `Include` is a **later** smoothness feature ([impl](../impl/README.md) Era 4), not a prerequisite for E2E.

## v1 commands

Illustrative; match plan gesture.

| Command | Behavior |
|---------|----------|
| `wt new <source> <name>` | `POST` create; wait or poll until `Running` or `Error`; on success **print** Host snippet + `ssh <name>` hint |
| `wt ls` | `GET` list; table name / status / ssh target |
| `wt rm <name>` | `DELETE`; **print** “remove Host \<name\>” guidance (later: apply removal) |
| `wt config` / flags | Agent URL, token (defaults sane) |

No `wt sh` required if Host + `RemoteCommand`/sshd setup is enough—optional sugar later.

## SSH config

**Early (Era 1–3):** print only, e.g.

```text
# add to ~/.ssh/config (or Include file):
Host <name>
  HostName <guest-ip-or-dns>
  User <world-user>
  IdentityFile <key>
```

**Later (when stable):** managed `Include` file or block—write on `new`, remove on `rm`. User never required to trust auto-edit before then.

Skip fancy `RemoteCommand` (byobu) until landing polish.

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

- Agent unreachable → clear error; nothing to print for Host.  
- Create fails mid-provision → surface agent error; `ls` shows `Error`; `rm` still cleans.  
- When auto-edit exists later: no half-written Host (roll back); until then print path has no file consistency risk.

## Out of scope for CLI (v1)

- Provider-specific flags (`--libvirt-pool`, kubecontext) beyond choosing agent URL  
- Browser tunnels  
- Implementing a second protocol for k8s—same HTTP API when that agent exists  

## One-line summary

**Thin Rust CLI: talk to agent, print (later apply) `Host <name>`, get out of the way for `ssh`.**
