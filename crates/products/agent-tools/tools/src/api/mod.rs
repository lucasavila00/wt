//! Typed GitHub and GitLab API operations.
//!
//! GraphQL custom scalar types must keep the names declared by the provider
//! schemas. Those names include capitalized acronyms, so this module allows the
//! corresponding Clippy lint once instead of annotating every scalar newtype.

#![allow(
    clippy::upper_case_acronyms,
    reason = "GraphQL custom scalar names are imposed by the provider schemas"
)]

mod cli;
mod github;
mod gitlab;
mod http;
#[cfg(test)]
mod test_server;

pub use cli::{render_cli_command_output, render_cli_confirmation};

use crate::ProviderKind;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

const CI_JOB_LOG_TAIL_LIMIT: usize = 1024 * 1024;
const CI_JOB_LOG_TRUNCATION_NOTICE: &str = "[earlier CI log output omitted]\n";

pub struct ProviderCommandScope<'a> {
    pub project: &'a str,
    pub base: &'a str,
    pub prefix: &'a str,
    pub branch: &'a str,
    pub head: &'a str,
}

pub struct ProviderProjectScope<'a> {
    pub host: &'a str,
    pub project: &'a str,
    pub prefix: &'a str,
}

include!(concat!(env!("OUT_DIR"), "/wt_tools_command.rs"));

pub const TYPESCRIPT_COMMAND_TYPE: &str = include_str!("wt-tools-command.ts");

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictState {
    Pending,
    Clean,
    Conflicting,
}

#[allow(
    dead_code,
    reason = "contextual variants remain for shared provider implementations and private tests"
)]
#[derive(Debug, Eq, PartialEq)]
pub enum ProviderCommand {
    ReadCurrentStatus,
    OpenChangeRequest,
    MarkChangeRequestReady {
        confirm_merged: bool,
    },
    MarkChangeRequestDraft {
        confirm_merged: bool,
    },
    AddChangeRequestComment {
        body: String,
        confirm_merged: bool,
    },
    EditChangeRequest {
        title: Option<String>,
        body: Option<String>,
        confirm_merged: bool,
    },
    ReadReviewThreads,
    ReplyToReviewThread {
        thread: ReviewThreadHandle,
        body: String,
        confirm_merged: bool,
    },
    SetReviewThreadResolved {
        thread: ReviewThreadHandle,
        resolved: bool,
        confirm_merged: bool,
    },
    ReadCiJobs,
    ReadCiJobLog {
        job: CiJobHandle,
    },
    RetryCiJob {
        job: CiJobHandle,
    },
    CancelCiJob {
        job: CiJobHandle,
    },
    WaitForReviewOrCiChange,
    CloseChangeRequest {
        confirm_merged: bool,
    },
    ReopenChangeRequest {
        confirm_merged: bool,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ChangeRequestStatus {
    pub handle: String,
    pub url: String,
    pub title: String,
    pub body: Option<String>,
    pub state: String,
    pub draft: bool,
    pub head: String,
    pub base: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conflict_state: Option<ConflictState>,
    pub review_state: Option<String>,
    pub threads: Vec<ReviewThread>,
    pub jobs: Vec<CiJob>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ReviewThread {
    pub handle: ReviewThreadHandle,
    pub resolvable: bool,
    pub resolved: bool,
    pub path: Option<String>,
    pub line: Option<u64>,
    pub comments: Vec<ReviewComment>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ReviewComment {
    pub author: String,
    pub body: String,
    pub url: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CiJob {
    pub handle: CiJobHandle,
    pub run: Option<String>,
    pub name: String,
    pub state: String,
    pub url: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CiRun {
    pub handle: String,
    pub name: String,
    pub state: String,
    pub trigger: Option<String>,
    pub url: Option<String>,
    pub head: String,
    pub branch: Option<String>,
}

// Every identifier newtype serializes as its underlying scalar.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ReviewThreadHandle(String);

impl ReviewThreadHandle {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ReviewThreadHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct CiJobHandle(String);

impl CiJobHandle {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for CiJobHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum ProviderCommandOutput {
    CurrentStatus(Option<ChangeRequestStatus>),
    ChangeRequest(ChangeRequestStatus),
    ReviewThreads(Vec<ReviewThread>),
    CiJobs(Vec<CiJob>),
    CiRun(CiRun),
    CiRunsAndJobs {
        runs: Vec<CiRun>,
        jobs: Vec<CiJob>,
    },
    CiJob(CiJob),
    CiJobLog {
        log: String,
        truncated: bool,
    },
    WaitTimeout {
        resource: String,
        last_state: String,
    },
    Confirmation(String),
}

pub trait GitProviderApi {
    fn verify_repository_access(&self, project: &str, base: &str) -> Result<()>;

    fn execute_command(
        &self,
        scope: &ProviderCommandScope<'_>,
        command: &ProviderCommand,
    ) -> Result<ProviderCommandOutput>;

    fn execute_cli_command(
        &self,
        scope: &ProviderProjectScope<'_>,
        command: &GitHostingCommand,
    ) -> Result<ProviderCommandOutput>;
}

const DEFAULT_CLI_WAIT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);
const CLI_WAIT_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(10);

fn cli_wait_deadline(timeout_seconds: Option<u64>) -> std::time::Instant {
    std::time::Instant::now()
        + timeout_seconds
            .map(std::time::Duration::from_secs)
            .unwrap_or(DEFAULT_CLI_WAIT_TIMEOUT)
}

fn wait_for_next_cli_poll(deadline: std::time::Instant) -> bool {
    let remaining = deadline.saturating_duration_since(std::time::Instant::now());
    if remaining.is_zero() {
        return false;
    }
    std::thread::sleep(remaining.min(CLI_WAIT_POLL_INTERVAL));
    std::time::Instant::now() < deadline
}

pub fn verify_provider_access(
    kind: ProviderKind,
    token_file: &Path,
    host: &str,
    project: &str,
    base: &str,
) -> Result<()> {
    let result = (|| {
        let token = read_provider_token(token_file)?;
        match kind {
            ProviderKind::GitHub => {
                github::GithubApi::new(host, &token)?.verify_repository_access(project, base)
            }
            ProviderKind::GitLab => {
                gitlab::GitlabApi::new(host, &token)?.verify_repository_access(project, base)
            }
        }
    })();
    result.with_context(|| {
        format!(
            "the {} API credential cannot prepare project {project} with base {base}",
            provider_name(kind)
        )
    })
}

fn wait_for_review_or_ci_change(
    provider: &impl GitProviderApi,
    scope: &ProviderCommandScope<'_>,
) -> Result<ProviderCommandOutput> {
    let read_status = ProviderCommand::ReadCurrentStatus;
    let initial = provider.execute_command(scope, &read_status)?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5 * 60);
    loop {
        std::thread::sleep(std::time::Duration::from_secs(10));
        let current = provider.execute_command(scope, &read_status)?;
        if current != initial {
            return Ok(current);
        }
        if std::time::Instant::now() >= deadline {
            bail!(
                "review and CI did not change within five minutes; run an appropriate show or list JSON action for the current status, or repeat the wait action"
            );
        }
    }
}

pub fn execute_cli_provider_command(
    kind: ProviderKind,
    token_file: &Path,
    scope: &ProviderProjectScope<'_>,
    command: &GitHostingCommand,
) -> Result<ProviderCommandOutput> {
    let result = (|| {
        let token = read_provider_token(token_file)?;
        match kind {
            ProviderKind::GitHub => {
                github::GithubApi::new(scope.host, &token)?.execute_cli_command(scope, command)
            }
            ProviderKind::GitLab => {
                gitlab::GitlabApi::new(scope.host, &token)?.execute_cli_command(scope, command)
            }
        }
    })();
    with_cli_command_context(result, kind, scope, command)
}

pub fn execute_cli_provider_command_at_base(
    kind: ProviderKind,
    token_file: &Path,
    base_url: &str,
    scope: &ProviderProjectScope<'_>,
    command: &GitHostingCommand,
) -> Result<ProviderCommandOutput> {
    let result = (|| {
        let token = read_provider_token(token_file)?;
        match kind {
            ProviderKind::GitHub => github::GithubApi::with_base_url(base_url.to_owned(), &token)?
                .execute_cli_command(scope, command),
            ProviderKind::GitLab => gitlab::GitlabApi::with_base_url(base_url.to_owned(), &token)?
                .execute_cli_command(scope, command),
        }
    })();
    with_cli_command_context(result, kind, scope, command)
}

fn with_cli_command_context(
    result: Result<ProviderCommandOutput>,
    kind: ProviderKind,
    scope: &ProviderProjectScope<'_>,
    command: &GitHostingCommand,
) -> Result<ProviderCommandOutput> {
    result.with_context(|| {
        format!(
            "wt-tools could not {}\nProvider: {} ({})\nRepository: {}\nResource: {}\nCause",
            command.action(),
            provider_name(kind),
            scope.host,
            scope.project,
            command.resource()
        )
    })
}

fn read_provider_token(token_file: &Path) -> Result<String> {
    let token = std::fs::read_to_string(token_file)
        .map_err(|error| anyhow::anyhow!("read provider API credential: {error}"))?;
    let token = token.trim();
    if token.is_empty() {
        bail!("provider API credential is empty");
    }
    Ok(token.to_owned())
}

pub fn provider_name(kind: ProviderKind) -> &'static str {
    match kind {
        ProviderKind::GitHub => "GitHub",
        ProviderKind::GitLab => "GitLab",
    }
}

fn attributed_comment(scope: &ProviderCommandScope<'_>, body: &str) -> String {
    format!(
        "{body}\n\n— WT world `{}`",
        scope.prefix.trim_end_matches('/')
    )
}

pub fn attributed_project_comment(scope: &ProviderProjectScope<'_>, body: &str) -> String {
    format!(
        "{body}\n\n— WT world `{}`",
        scope.prefix.trim_end_matches('/')
    )
}

#[cfg(test)]
mod tests;
