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

pub(crate) use cli::render_cli_command_output;
#[cfg(test)]
use cli::{render_threads, tail_ci_job_log_at_limit};

use crate::ProviderKind;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

const CI_JOB_LOG_TAIL_LIMIT: usize = 64 * 1024;
const CI_JOB_LOG_TRUNCATION_NOTICE: &str = "[earlier CI log output omitted]\n";

pub(crate) struct ProviderCommandScope<'a> {
    pub project: &'a str,
    pub base: &'a str,
    pub prefix: &'a str,
    pub branch: &'a str,
    pub head: &'a str,
}

pub(crate) struct ProviderProjectScope<'a> {
    pub host: &'a str,
    pub project: &'a str,
    pub base: &'a str,
    pub prefix: &'a str,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ChangeRequestState {
    Ready,
    Draft,
    Open,
    Closed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum CliCommand {
    ShowMr {
        mr: u64,
    },
    ShowMrForBranch {
        branch: String,
    },
    ShowRun {
        run: u64,
    },
    ShowJob {
        job: u64,
    },
    ListThreads {
        mr: u64,
    },
    ListCi {
        commit: String,
    },
    ListJobs {
        run: u64,
    },
    LogJob {
        job: u64,
    },
    WaitMr {
        mr: u64,
    },
    WaitRun {
        run: u64,
    },
    WaitJob {
        job: u64,
    },
    OpenMr {
        head: String,
        base: String,
        #[serde(default)]
        draft: bool,
    },
    SetMr {
        mr: u64,
        state: ChangeRequestState,
    },
    EditMr {
        mr: u64,
        title: Option<String>,
        body: Option<String>,
    },
    CommentMr {
        mr: u64,
        body: String,
    },
    ReplyThread {
        mr: u64,
        thread: ReviewThreadHandle,
        body: String,
    },
    SetThread {
        mr: u64,
        thread: ReviewThreadHandle,
        resolved: bool,
    },
    RetryJob {
        job: u64,
    },
    CancelJob {
        job: u64,
    },
    CancelRun {
        run: u64,
    },
    ReportAgGitBug {
        description: String,
    },
    ReportAgGitIssue {
        description: String,
    },
    SuggestAgGitImprovement {
        description: String,
    },
    RequestAgGitFeature {
        description: String,
    },
}

#[allow(
    dead_code,
    reason = "contextual variants remain for shared provider implementations and private tests"
)]
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum ProviderCommand {
    ReadCurrentStatus,
    OpenChangeRequest {
        draft: bool,
    },
    MarkChangeRequestReady,
    MarkChangeRequestDraft,
    AddChangeRequestComment {
        body: String,
    },
    EditChangeRequest {
        title: Option<String>,
        body: Option<String>,
    },
    ReadReviewThreads,
    ReplyToReviewThread {
        thread: ReviewThreadHandle,
        body: String,
    },
    SetReviewThreadResolved {
        thread: ReviewThreadHandle,
        resolved: bool,
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
    CloseChangeRequest,
    ReopenChangeRequest,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ChangeRequestStatus {
    pub handle: String,
    pub url: String,
    pub title: String,
    pub state: String,
    pub draft: bool,
    pub head: String,
    pub base: String,
    pub review_state: Option<String>,
    pub threads: Vec<ReviewThread>,
    pub jobs: Vec<CiJob>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ReviewThread {
    pub handle: ReviewThreadHandle,
    pub resolvable: bool,
    pub resolved: bool,
    pub path: Option<String>,
    pub line: Option<u64>,
    pub comments: Vec<ReviewComment>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ReviewComment {
    pub author: String,
    pub body: String,
    pub url: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CiJob {
    pub handle: CiJobHandle,
    pub run: Option<String>,
    pub name: String,
    pub state: String,
    pub url: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CiRun {
    pub handle: String,
    pub name: String,
    pub state: String,
    pub url: Option<String>,
    pub head: String,
    pub branch: Option<String>,
}

// Every identifier newtype serializes as its underlying scalar.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub(crate) struct ReviewThreadHandle(String);

impl ReviewThreadHandle {
    pub(crate) fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub(crate) fn as_str(&self) -> &str {
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
pub(crate) struct CiJobHandle(String);

impl CiJobHandle {
    pub(crate) fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for CiJobHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum ProviderCommandOutput {
    CurrentStatus(Option<ChangeRequestStatus>),
    ChangeRequest(ChangeRequestStatus),
    ReviewThreads(Vec<ReviewThread>),
    CiJobs(Vec<CiJob>),
    CiRun(CiRun),
    CiRunsAndJobs { runs: Vec<CiRun>, jobs: Vec<CiJob> },
    CiJob(CiJob),
    CiJobLog(String),
    Confirmation(String),
}

pub(crate) trait GitProviderApi {
    fn verify_repository_access(&self, project: &str, base: &str) -> Result<()>;

    fn execute_command(
        &self,
        scope: &ProviderCommandScope<'_>,
        command: &ProviderCommand,
    ) -> Result<ProviderCommandOutput>;

    fn execute_cli_command(
        &self,
        scope: &ProviderProjectScope<'_>,
        command: &CliCommand,
    ) -> Result<ProviderCommandOutput>;
}

pub(crate) fn verify_provider_access(
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

pub(crate) fn execute_cli_provider_command(
    kind: ProviderKind,
    token_file: &Path,
    scope: &ProviderProjectScope<'_>,
    command: &CliCommand,
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

pub(crate) fn execute_cli_provider_command_at_base(
    kind: ProviderKind,
    token_file: &Path,
    base_url: &str,
    scope: &ProviderProjectScope<'_>,
    command: &CliCommand,
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
    command: &CliCommand,
) -> Result<ProviderCommandOutput> {
    result.with_context(|| {
        format!(
            "ag-git could not {}\nProvider: {} ({})\nProject: {}\nResource: {}\nCause",
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

fn provider_name(kind: ProviderKind) -> &'static str {
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

pub(crate) fn attributed_project_comment(scope: &ProviderProjectScope<'_>, body: &str) -> String {
    format!(
        "{body}\n\n— WT world `{}`",
        scope.prefix.trim_end_matches('/')
    )
}

#[cfg(test)]
mod tests;
