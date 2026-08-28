mod service;

use crate::{
    ClientOperation, ControlRequest, ControlResponse, DuplexStream, GitService, Grant,
    TransportRequest, TransportResponse, BRANCH_PREFIX, PROTOCOL_VERSION,
};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use uuid::Uuid;
use wt_git_smart_protocol::{
    serve_git, successful_push_updates, GitTarget, HostKeyPolicy, PushViolation, WritePolicy,
};
use wt_tools::{self as api, ProviderKind};
use wt_world::WorldId;

#[derive(Clone, Debug)]
pub struct GatewayConfig {
    pub state_file: PathBuf,
    pub database_path: PathBuf,
    pub providers: Vec<Provider>,
}

#[derive(Clone, Debug)]
pub enum Provider {
    Ssh {
        kind: ProviderKind,
        host: String,
        user: String,
        port: Option<u16>,
        api_token_file: PathBuf,
        private_key_file: PathBuf,
    },
    Local {
        host: String,
        repositories: PathBuf,
        api: Option<FixtureApi>,
    },
}

#[derive(Clone, Debug)]
pub struct FixtureApi {
    pub kind: ProviderKind,
    pub base_url: String,
    pub token_file: PathBuf,
}

impl Provider {
    fn host(&self) -> &str {
        match self {
            Self::Ssh { host, .. } | Self::Local { host, .. } => host,
        }
    }

    fn api_kind(&self) -> Option<ProviderKind> {
        match self {
            Self::Ssh { kind, .. } => Some(*kind),
            Self::Local { api, .. } => api.as_ref().map(|api| api.kind),
        }
    }
}

#[derive(Clone)]
pub struct Gateway {
    config: GatewayConfig,
    state: Arc<Mutex<State>>,
    pane_observations: Arc<Mutex<PaneObservations>>,
}

#[derive(Default)]
struct PaneObservations {
    snapshots: std::collections::BTreeMap<WorldId, Vec<PaneObservationSnapshot>>,
    inactive_worlds: std::collections::BTreeSet<WorldId>,
    generations: std::collections::BTreeMap<WorldId, u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaneObservationSnapshot {
    pub tmux_session: String,
    pub pane_id: String,
    screen_fingerprint: String,
    pub cwd: String,
    pub git_branch: Option<String>,
    pub changed_at_unix_ms: i64,
    pub observed_at_unix_ms: i64,
    pub render: wt_control_protocol::PaneRender,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct State {
    grants: Vec<GrantRecord>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct GrantRecord {
    id: String,
    token: String,
    world_id: String,
    revoked: bool,
}

struct AuthorizedGrant {
    record: GrantRecord,
    pane_generation: u64,
}

fn cli_unavailable() -> String {
    "wtg tools: provider API commands are not available for this project.\nNormal Git fetch, pull, and push are available.\n".to_owned()
}

fn valid_host(value: &str) -> bool {
    !value.is_empty()
        && value == value.to_ascii_lowercase()
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-')
        })
}

fn git_context_header(source: &str) -> String {
    let project = parse_source(source)
        .map(|source| source.path.trim_end_matches(".git").to_owned())
        .unwrap_or_else(|_| source.to_owned());
    format!(
        "remote: This is a WT-managed development environment for a coding agent.\n\
remote: The gateway does not expose developer SSH keys or provider credentials.\n\
remote: Do not look for credentials or use gh or glab.\n\
remote: WT gives you read access to every repository available to this gateway.\n\
remote: This Git operation is for project {project}.\n\
remote: Use normal Git for commits, fetches, pulls, and pushes.\n\
remote: Every WT world can write branches under {BRANCH_PREFIX}.\n\
remote: wtg tools uses explicit provider resource types and IDs; it does not infer\n\
remote: resources from the current checkout.\n\
remote: Run wtg tools --help to discover every available command.\n\
remote:\n"
    )
}

fn world_prompt() -> String {
    format!(
        "This process runs as the non-root `wt` user inside a disposable Ubuntu 24.04 WT KVM guest. The guest is the security boundary, so installing system packages, compilers, package managers, language runtimes, and test dependencies is allowed. Use `sudo apt-get update` and `sudo apt-get install -y PACKAGE` to install missing system prerequisites. If Rust tooling is missing, install stable Rust as the normal user with rustup, not apt, then source `$HOME/.cargo/env` and add the rustfmt and clippy components. If other required tooling is missing, install it instead of skipping validation. System-level changes inside the guest are acceptable. Run the repository's normal build, lint, typecheck, and test workflow whenever practical.\n\nThis environment has wtg tools installed for pull or merge request, review, and CI operations; run wtg tools help to see its supported commands. Use normal Git for commits, fetches, pulls, and pushes. The Git gateway can read every available repository and requires branch names to use the shared `{BRANCH_PREFIX}` prefix (for example, `{BRANCH_PREFIX}fix-login`). Every WT world may update, force-push, or delete any branch under `{BRANCH_PREFIX}`, so an agent can continue or take over work from another agent. If the gateway rejects a branch, rename it with git branch -m {BRANCH_PREFIX}NAME.\n"
    )
}

fn validate_repository(repository: &str) -> Result<()> {
    if repository.is_empty()
        || repository.starts_with('/')
        || repository
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        || !repository
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'_' | b'-'))
    {
        bail!("invalid Git repository");
    }
    Ok(())
}

fn normalize_repository(repository: &str) -> String {
    repository
        .strip_suffix(".git")
        .unwrap_or(repository)
        .to_owned()
}

struct GitSource {
    host: String,
    user: String,
    port: Option<u16>,
    path: String,
}

fn parse_source(value: &str) -> Result<GitSource> {
    let (user, host_port, path) = if let Some(rest) = value.strip_prefix("ssh://") {
        let (authority, path) = rest
            .split_once('/')
            .ok_or_else(|| anyhow::anyhow!("invalid SSH Git source"))?;
        let (user, host_port) = authority
            .split_once('@')
            .ok_or_else(|| anyhow::anyhow!("SSH Git source must include a user"))?;
        (user, host_port, path)
    } else {
        let (authority, path) = value
            .split_once(':')
            .ok_or_else(|| anyhow::anyhow!("invalid SSH Git source"))?;
        let (user, host) = authority
            .split_once('@')
            .ok_or_else(|| anyhow::anyhow!("invalid SSH Git source"))?;
        (user, host, path)
    };
    let (host, port) = match host_port.rsplit_once(':') {
        Some((host, port)) if port.bytes().all(|byte| byte.is_ascii_digit()) => (
            host,
            Some(port.parse::<u16>().context("invalid SSH Git port")?),
        ),
        _ => (host_port, None),
    };
    let host = host.to_ascii_lowercase();
    if user.is_empty()
        || !valid_host(&host)
        || path.is_empty()
        || path.starts_with('/')
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        || !path
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'_' | b'-'))
    {
        bail!("invalid SSH Git source");
    }
    Ok(GitSource {
        host,
        user: user.to_owned(),
        port,
        path: path.to_owned(),
    })
}

fn git_target<'a>(provider: &'a Provider, source: &'a GitSource) -> Result<GitTarget<'a>> {
    match provider {
        Provider::Local { repositories, .. } => Ok(GitTarget::Local {
            repositories,
            path: &source.path,
        }),
        Provider::Ssh {
            user,
            port,
            private_key_file,
            ..
        } => {
            if user != &source.user || port != &source.port {
                bail!("Git source does not match the configured SSH endpoint");
            }
            Ok(GitTarget::Ssh {
                host: &source.host,
                user: &source.user,
                port: source.port,
                private_key_file,
                host_key_policy: HostKeyPolicy::AcceptAny,
                path: &source.path,
            })
        }
    }
}

const HELP_PREFIX: &str = "\
wtg tools reads and changes explicitly identified Git provider resources and records\n\
feedback about wtg tools itself. It accepts exactly one JSON command object and\n\
rejects unknown fields.\n\
\n\
USAGE:\n\
    wtg tools '<JSON>'\n\
    wtg tools --file PATH\n\
    wtg tools --stdin\n\
    wtg tools -\n\
\n\
`--file PATH` reads one UTF-8 JSON command from a file. `--file -`, `--stdin`, and\n\
`-` read one UTF-8 JSON command from standard input until end-of-file.\n\
\n\
TYPESCRIPT COMMAND TYPE:\n";

const HELP_SUFFIX: &str = "\
\n\
EXAMPLE:\n\
    wtg tools '{\"target\":{\"provider\":\"github\",\"repository\":\"acme/widget\"},\"command\":{\"action\":\"show_mr_for_branch\",\"branch\":\"wt/fix-login\"}}'\n\
    wtg tools --file command.json\n\
    printf '%s\\n' '{\"command\":{\"action\":\"report_wt_tool_issue\",\"description\":\"example\"}}' | wtg tools -\n\
\n\
`show_mr_for_branch` returns the single open MR from the named branch in the target\n\
repository. It fails when there is no match or multiple matches.\n\
\n\
The four wtg tools feedback actions omit `target` and store feedback against this world\n\
without contacting the Git provider.\n\
\n\
Provider operations return one JSON result or error object. Help remains\n\
plain text.\n\
\n\
The target selects one installer-configured provider and an explicit repository.\n\
IDs must be positive integers. Commit values must be 7 to\n\
64 hexadecimal characters. Use normal Git for commits, fetches, pulls, and pushes.\n";

pub fn wt_tools_help() -> String {
    let command_type = api::TYPESCRIPT_COMMAND_TYPE
        .trim_end()
        .lines()
        .map(|line| {
            if line.is_empty() {
                String::new()
            } else {
                format!("    {line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!("{HELP_PREFIX}{command_type}\n{HELP_SUFFIX}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_prompt_explains_the_disposable_world_and_wtg_tools() {
        insta::assert_snapshot!(world_prompt(), @r###"
        This process runs as the non-root `wt` user inside a disposable Ubuntu 24.04 WT KVM guest. The guest is the security boundary, so installing system packages, compilers, package managers, language runtimes, and test dependencies is allowed. Use `sudo apt-get update` and `sudo apt-get install -y PACKAGE` to install missing system prerequisites. If Rust tooling is missing, install stable Rust as the normal user with rustup, not apt, then source `$HOME/.cargo/env` and add the rustfmt and clippy components. If other required tooling is missing, install it instead of skipping validation. System-level changes inside the guest are acceptable. Run the repository's normal build, lint, typecheck, and test workflow whenever practical.

        This environment has wtg tools installed for pull or merge request, review, and CI operations; run wtg tools help to see its supported commands. Use normal Git for commits, fetches, pulls, and pushes. The Git gateway can read every available repository and requires branch names to use the shared `wt/` prefix (for example, `wt/fix-login`). Every WT world may update, force-push, or delete any branch under `wt/`, so an agent can continue or take over work from another agent. If the gateway rejects a branch, rename it with git branch -m wt/NAME.
        "###);
    }

    #[test]
    fn help_is_the_complete_command_contract() {
        insta::with_settings!({snapshot_path => "gateway/snapshots"}, {
            insta::assert_snapshot!(wt_tools_help());
        });
    }
}
