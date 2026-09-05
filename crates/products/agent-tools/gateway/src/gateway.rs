mod service;

use crate::{
    ClientOperation, ControlRequest, ControlResponse, DuplexStream, GitService, TransportRequest,
    TransportResponse, BRANCH_PREFIX, PROTOCOL_VERSION,
};
use anyhow::{bail, Context, Result};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use uuid::Uuid;
use wt_git_smart_protocol::{
    serve_git, successful_push_updates, GitTarget, HostKeyPolicy, PushViolation, WritePolicy,
};
use wt_tools::{self as api, ProviderKind};
use wt_world::WorldId;

#[derive(Clone, Debug)]
pub struct GatewayConfig {
    pub providers: Vec<Provider>,
}

#[derive(Clone)]
pub struct ActivityRecorder {
    registry: Arc<Mutex<wt_workload_registry::Registry>>,
}

impl ActivityRecorder {
    pub fn open(path: &Path) -> Result<Self> {
        Ok(Self {
            registry: Arc::new(Mutex::new(
                wt_workload_registry::Registry::open(path).context("open WT activity registry")?,
            )),
        })
    }

    fn registry(&self) -> Result<std::sync::MutexGuard<'_, wt_workload_registry::Registry>> {
        self.registry
            .lock()
            .map_err(|_| anyhow::anyhow!("activity registry lock poisoned"))
    }

    fn record_git_activity(&self, input: wt_workload_registry::GitActivityInput<'_>) -> Result<()> {
        self.registry()?.insert_git_activity(input)?;
        Ok(())
    }

    fn record_wt_tools_activity(
        &self,
        input: wt_workload_registry::WtToolsActivityInput<'_>,
    ) -> Result<()> {
        self.registry()?.insert_wt_tools_activity(input)?;
        Ok(())
    }

    fn record_agent_tool_report(
        &self,
        world_id: WorldId,
        kind: wt_workload_registry::AgentToolReportKind,
        description: &str,
    ) -> Result<()> {
        self.registry()?
            .insert_agent_tool_report(world_id, kind, description)?;
        Ok(())
    }
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
    activity: ActivityRecorder,
    world_state: Arc<Mutex<WorldState>>,
}

#[derive(Default)]
struct WorldState {
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

#[derive(Debug)]
struct AuthorizedWorld {
    world_id: WorldId,
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
remote: The developer's SSH keys and provider credentials are not available in this world.\n\
remote: Do not look for credentials or use gh or glab.\n\
remote: You can read every repository configured for access from WT worlds.\n\
remote: This Git operation is for project {project}.\n\
remote: Use normal Git for commits, fetches, pulls, and pushes.\n\
remote: Every WT world can write branches under {BRANCH_PREFIX}.\n\
remote: Pushes must preserve history; rewrites and branch deletions are rejected.\n\
remote: wtg tools uses explicit provider resource types and IDs; it does not infer\n\
remote: resources from the current checkout.\n\
remote: Run wtg tools --help to discover every available command.\n\
remote:\n"
    )
}

fn world_prompt() -> String {
    format!(
        "This process runs as the non-root `wt` user inside a disposable Ubuntu 24.04 WT KVM guest. The guest is the security boundary, so installing system packages, compilers, package managers, language runtimes, and test dependencies is allowed. Use `sudo apt-get update` and `sudo apt-get install -y PACKAGE` to install missing system prerequisites. If Rust tooling is missing, install stable Rust as the normal user with rustup, not apt, then source `$HOME/.cargo/env` and add the rustfmt and clippy components. If other required tooling is missing, install it instead of skipping validation. System-level changes inside the guest are acceptable. Run the repository's normal build, lint, typecheck, and test workflow whenever practical.\n\nThis environment has wtg tools installed for pull or merge request, review, and CI operations; run wtg tools help to see its supported commands. Use normal Git for commits, fetches, pulls, and pushes. From this world, you can read every repository configured for access from WT worlds. Branches you push must use the shared `{BRANCH_PREFIX}` prefix (for example, `{BRANCH_PREFIX}fix-login`). Every WT world may create branches and push fast-forward updates under `{BRANCH_PREFIX}`, so an agent can continue or take over work from another agent. History rewrites and branch deletions are blocked from this world. If your branch name does not use this prefix, rename it with git branch -m {BRANCH_PREFIX}NAME.\n\nPRs are usually squash-merged. Preserve the PR's development and review history with follow-up commits; do not rebase, amend published commits, or force-push as routine cleanup. Merge the base branch into the PR branch when needed. If a history rewrite is needed, prepare and push the replacement commits to a new `{BRANCH_PREFIX}` branch, then ask the user to update the original PR branch from their laptop using their own credentials. Provide the repository, both branch names, the original branch's expected remote SHA, the replacement SHA, and exact fetch and push commands using `--force-with-lease=<ref>:<expected-old-sha>` and the replacement SHA as the push source. If the original branch moves, reassess instead of refreshing the lease blindly.\n"
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

        This environment has wtg tools installed for pull or merge request, review, and CI operations; run wtg tools help to see its supported commands. Use normal Git for commits, fetches, pulls, and pushes. From this world, you can read every repository configured for access from WT worlds. Branches you push must use the shared `wt/` prefix (for example, `wt/fix-login`). Every WT world may create branches and push fast-forward updates under `wt/`, so an agent can continue or take over work from another agent. History rewrites and branch deletions are blocked from this world. If your branch name does not use this prefix, rename it with git branch -m wt/NAME.

        PRs are usually squash-merged. Preserve the PR's development and review history with follow-up commits; do not rebase, amend published commits, or force-push as routine cleanup. Merge the base branch into the PR branch when needed. If a history rewrite is needed, prepare and push the replacement commits to a new `wt/` branch, then ask the user to update the original PR branch from their laptop using their own credentials. Provide the repository, both branch names, the original branch's expected remote SHA, the replacement SHA, and exact fetch and push commands using `--force-with-lease=<ref>:<expected-old-sha>` and the replacement SHA as the push source. If the original branch moves, reassess instead of refreshing the lease blindly.
        "###);
    }

    #[test]
    fn help_is_the_complete_command_contract() {
        insta::with_settings!({snapshot_path => "gateway/snapshots"}, {
            insta::assert_snapshot!(wt_tools_help());
        });
    }
}
