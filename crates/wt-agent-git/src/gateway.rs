mod service;

use crate::{
    api, ClientOperation, ControlRequest, ControlResponse, DuplexStream, GitService, Grant,
    Repository, TransportRequest, TransportResponse, BRANCH_PREFIX, PROTOCOL_VERSION,
};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use uuid::Uuid;
use wt_git_core::{
    repository_refs, serve_git, successful_push_updates, write_packet, GitTarget, PushViolation,
    WritePolicy,
};

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
        known_hosts_file: PathBuf,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderKind {
    GitHub,
    GitLab,
}

impl Provider {
    fn host(&self) -> &str {
        match self {
            Self::Ssh { host, .. } | Self::Local { host, .. } => host,
        }
    }
}

#[derive(Clone)]
pub struct Gateway {
    config: GatewayConfig,
    state: Arc<Mutex<State>>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct State {
    grants: Vec<GrantRecord>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct GrantRecord {
    id: String,
    token: String,
    world_id: String,
    #[serde(default, rename = "source", skip_serializing_if = "Option::is_none")]
    legacy_source: Option<String>,
    #[serde(default, rename = "base", skip_serializing_if = "Option::is_none")]
    legacy_base: Option<String>,
    #[serde(default, rename = "prefix", skip_serializing_if = "Option::is_none")]
    legacy_prefix: Option<String>,
    revoked: bool,
}

impl GrantRecord {
    fn is_legacy_scoped(&self) -> bool {
        self.legacy_source.is_some() || self.legacy_base.is_some() || self.legacy_prefix.is_some()
    }
}

fn cli_unavailable() -> String {
    "ag-git: provider API commands are not available for this project.\nNormal Git fetch, pull, and push are available.\n".to_owned()
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
remote: ag-git uses explicit provider resource types and IDs; it does not infer\n\
remote: resources from the current checkout.\n\
remote: Run ag-git --help to discover every available command.\n\
remote:\n"
    )
}

fn validate_repository(repository: &Repository) -> Result<()> {
    if !valid_host(&repository.host)
        || repository.project.is_empty()
        || repository.project.starts_with('/')
        || repository.project.ends_with(".git")
        || repository
            .project
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        || !repository
            .project
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'_' | b'-'))
    {
        bail!("invalid Git repository");
    }
    Ok(())
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
    if user.is_empty()
        || !valid_host(host)
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
        host: host.to_owned(),
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
            known_hosts_file,
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
                known_hosts_file,
                path: &source.path,
            })
        }
    }
}

fn verify_repository(provider: &Provider, source: &GitSource, base: &str) -> Result<()> {
    let refs = repository_refs(git_target(provider, source)?).with_context(|| {
        format!(
            "the gateway SSH key cannot read repository {} on {}",
            source.path, source.host
        )
    })?;
    let expected = format!("refs/heads/{base}");
    if !refs.iter().any(|(_, reference)| reference == &expected) {
        bail!("Git base branch `{base}` does not exist");
    }
    if let Provider::Ssh {
        kind,
        api_token_file,
        ..
    } = provider
    {
        api::verify_provider_access(
            *kind,
            api_token_file,
            &source.host,
            source.path.trim_end_matches(".git"),
            base,
        )
        .context("verify provider API access")?;
    }
    Ok(())
}

const HELP: &str = "\
ag-git reads and changes explicitly identified Git provider resources and records\n\
feedback about ag-git itself. It accepts exactly one JSON command object and\n\
rejects unknown fields.\n\
\n\
USAGE:\n\
    ag-git '<JSON>'\n\
\n\
TYPESCRIPT COMMAND TYPE:\n\
    type AgGitCommand =\n\
      | { action: \"show_mr\"; mr: number }\n\
      | { action: \"show_mr_for_branch\"; branch: string }\n\
      | { action: \"show_run\"; run: number }\n\
      | { action: \"show_job\"; job: number }\n\
      | { action: \"list_threads\"; mr: number }\n\
      | { action: \"list_ci\"; commit: string }\n\
      | { action: \"list_jobs\"; run: number }\n\
      | { action: \"log_job\"; job: number }\n\
      | { action: \"wait_mr\"; mr: number }\n\
      | { action: \"wait_run\"; run: number }\n\
      | { action: \"wait_job\"; job: number }\n\
      | { action: \"open_mr\"; head: string; base: string; draft?: boolean }\n\
      | { action: \"set_mr\"; mr: number; state: \"ready\" | \"draft\" | \"open\" | \"closed\" }\n\
      | { action: \"edit_mr\"; mr: number; title?: string; body?: string }\n\
      | { action: \"comment_mr\"; mr: number; body: string }\n\
      | { action: \"reply_thread\"; mr: number; thread: string; body: string }\n\
      | { action: \"set_thread\"; mr: number; thread: string; resolved: boolean }\n\
      | { action: \"retry_job\"; job: number }\n\
      | { action: \"cancel_job\"; job: number }\n\
      | { action: \"cancel_run\"; run: number }\n\
      | { action: \"report_ag_git_bug\"; description: string }\n\
      | { action: \"report_ag_git_issue\"; description: string }\n\
      | { action: \"suggest_ag_git_improvement\"; description: string }\n\
      | { action: \"request_ag_git_feature\"; description: string };\n\
\n\
EXAMPLE:\n\
    ag-git '{\"action\":\"show_mr_for_branch\",\"branch\":\"wt/fix-login\"}'\n\
\n\
`show_mr_for_branch` returns the single open MR from the named branch to the\n\
gateway grant's base branch. It fails when there is no match or multiple matches.\n\
\n\
The four ag-git reporting actions store feedback against this authenticated world\n\
without contacting the Git provider.\n\
\n\
The provider and project come from this world's gateway grant. Every other\n\
resource is explicit. IDs must be positive integers. Commit values must be 7 to\n\
64 hexadecimal characters. Use normal Git for commits, fetches, pulls, and pushes.\n";

#[cfg(test)]
mod tests;
