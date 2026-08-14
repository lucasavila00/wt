//! Typed GitHub and GitLab API operations.
//!
//! GraphQL custom scalar types must keep the names declared by the provider
//! schemas. Those names include capitalized acronyms, so this module allows the
//! corresponding Clippy lint once instead of annotating every scalar newtype.

#![allow(
    clippy::upper_case_acronyms,
    reason = "GraphQL custom scalar names are imposed by the provider schemas"
)]

mod github;
mod gitlab;
mod http;
#[cfg(test)]
mod test_server;

use crate::ProviderKind;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

const CI_JOB_LOG_TAIL_LIMIT: usize = 64 * 1024;
const CI_JOB_LOG_TRUNCATION_NOTICE: &str = "[earlier CI log output omitted]\n";

pub(crate) struct ProviderCommandScope<'a> {
    pub host: &'a str,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ChangeRequestState {
    Ready,
    Draft,
    Open,
    Closed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum CliCommand {
    ShowMr {
        mr: u64,
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
}

#[allow(
    dead_code,
    reason = "contextual variants remain private provider test coverage for the post-push implementation"
)]
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum ProviderCommand {
    ReadCurrentStatus,
    ReadChangeRequestAfterPush,
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

fn with_provider_command_context(
    result: Result<ProviderCommandOutput>,
    kind: ProviderKind,
    scope: &ProviderCommandScope<'_>,
    command: &ProviderCommand,
) -> Result<ProviderCommandOutput> {
    result.with_context(|| {
        format!(
            "ag-git could not {}\nProvider: {} ({})\nProject: {}\nBranch: {}\nBase: {}\nCurrent commit: {}\nCause",
            command.action(),
            provider_name(kind),
            scope.host,
            scope.project,
            scope.branch,
            scope.base,
            scope.head
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
                "review and CI did not change within five minutes; run `ag-git` for the current status or `ag-git wait` to wait again"
            );
        }
    }
}

pub(crate) fn execute_provider_command(
    kind: ProviderKind,
    token_file: &Path,
    scope: &ProviderCommandScope<'_>,
    command: &ProviderCommand,
) -> Result<ProviderCommandOutput> {
    let result = (|| {
        let token = read_provider_token(token_file)?;
        match kind {
            ProviderKind::GitHub => {
                github::GithubApi::new(scope.host, &token)?.execute_command(scope, command)
            }
            ProviderKind::GitLab => {
                gitlab::GitlabApi::new(scope.host, &token)?.execute_command(scope, command)
            }
        }
    })();
    with_provider_command_context(result, kind, scope, command)
}

pub(crate) fn execute_provider_command_at_base(
    kind: ProviderKind,
    token_file: &Path,
    base_url: &str,
    scope: &ProviderCommandScope<'_>,
    command: &ProviderCommand,
) -> Result<ProviderCommandOutput> {
    let result = (|| {
        let token = read_provider_token(token_file)?;
        match kind {
            ProviderKind::GitHub => github::GithubApi::with_base_url(base_url.to_owned(), &token)?
                .execute_command(scope, command),
            ProviderKind::GitLab => gitlab::GitlabApi::with_base_url(base_url.to_owned(), &token)?
                .execute_command(scope, command),
        }
    })();
    with_provider_command_context(result, kind, scope, command)
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

impl ProviderCommand {
    fn action(&self) -> &'static str {
        match self {
            Self::ReadCurrentStatus => "read the current request, reviews, and CI status",
            Self::ReadChangeRequestAfterPush => "read the request updated by the Git push",
            Self::OpenChangeRequest { .. } => "open the pull or merge request",
            Self::MarkChangeRequestReady => "mark the pull or merge request ready",
            Self::MarkChangeRequestDraft => "mark the pull or merge request as draft",
            Self::AddChangeRequestComment { .. } => "add a pull or merge request comment",
            Self::EditChangeRequest { .. } => "edit the pull or merge request",
            Self::ReadReviewThreads => "read review threads",
            Self::ReplyToReviewThread { .. } => "reply to the review thread",
            Self::SetReviewThreadResolved { resolved: true, .. } => "resolve the review thread",
            Self::SetReviewThreadResolved {
                resolved: false, ..
            } => "reopen the review thread",
            Self::ReadCiJobs => "read CI jobs for the current commit",
            Self::ReadCiJobLog { .. } => "read the CI job log",
            Self::RetryCiJob { .. } => "retry the CI job",
            Self::CancelCiJob { .. } => "cancel the CI job",
            Self::WaitForReviewOrCiChange => "wait for review or CI to change",
            Self::CloseChangeRequest => "close the pull or merge request",
            Self::ReopenChangeRequest => "reopen the pull or merge request",
        }
    }
}

impl CliCommand {
    pub(crate) fn parse(args: &[String]) -> Result<Self> {
        let Some((name, rest)) = args.split_first() else {
            bail!("usage: ag-git COMMAND RESOURCE [ID]");
        };
        match name.as_str() {
            "show" => match rest {
                [kind, id] if kind == "mr" => Ok(Self::ShowMr {
                    mr: numeric(id, "MR")?,
                }),
                [kind, id] if kind == "run" => Ok(Self::ShowRun {
                    run: numeric(id, "run")?,
                }),
                [kind, id] if kind == "job" => Ok(Self::ShowJob {
                    job: numeric(id, "job")?,
                }),
                _ => bail!("usage: ag-git show mr|run|job ID"),
            },
            "list" => match rest {
                [items, parent, id] if items == "threads" && parent == "mr" => {
                    Ok(Self::ListThreads {
                        mr: numeric(id, "MR")?,
                    })
                }
                [items, parent, commit] if items == "ci" && parent == "commit" => {
                    validate_commit(commit)?;
                    Ok(Self::ListCi {
                        commit: commit.clone(),
                    })
                }
                [items, parent, id] if items == "jobs" && parent == "run" => Ok(Self::ListJobs {
                    run: numeric(id, "run")?,
                }),
                _ => bail!("usage: ag-git list threads mr ID | ci commit SHA | jobs run ID"),
            },
            "log" => match rest {
                [kind, id] if kind == "job" => Ok(Self::LogJob {
                    job: numeric(id, "job")?,
                }),
                _ => bail!("usage: ag-git log job ID"),
            },
            "wait" => match rest {
                [kind, id] if kind == "mr" => Ok(Self::WaitMr {
                    mr: numeric(id, "MR")?,
                }),
                [kind, id] if kind == "run" => Ok(Self::WaitRun {
                    run: numeric(id, "run")?,
                }),
                [kind, id] if kind == "job" => Ok(Self::WaitJob {
                    job: numeric(id, "job")?,
                }),
                _ => bail!("usage: ag-git wait mr|run|job ID"),
            },
            "open" => parse_open_mr(rest),
            "set" => parse_set(rest),
            "edit" => parse_explicit_edit(rest),
            "comment" => match rest {
                [kind, id, body @ ..] if kind == "mr" => Ok(Self::CommentMr {
                    mr: numeric(id, "MR")?,
                    body: required_text(body, "ag-git comment mr ID TEXT")?,
                }),
                _ => bail!("usage: ag-git comment mr ID TEXT"),
            },
            "reply" => parse_reply(rest),
            "retry" => match rest {
                [kind, id] if kind == "job" => Ok(Self::RetryJob {
                    job: numeric(id, "job")?,
                }),
                _ => bail!("usage: ag-git retry job ID"),
            },
            "cancel" => match rest {
                [kind, id] if kind == "job" => Ok(Self::CancelJob {
                    job: numeric(id, "job")?,
                }),
                [kind, id] if kind == "run" => Ok(Self::CancelRun {
                    run: numeric(id, "run")?,
                }),
                _ => bail!("usage: ag-git cancel job|run ID"),
            },
            _ => bail!("unknown command `{name}`; run `ag-git --help`"),
        }
    }

    fn action(&self) -> &'static str {
        match self {
            Self::ShowMr { .. } => "show the merge request",
            Self::ShowRun { .. } => "show the CI run",
            Self::ShowJob { .. } => "show the CI job",
            Self::ListThreads { .. } => "list merge request threads",
            Self::ListCi { .. } => "list CI for the commit",
            Self::ListJobs { .. } => "list jobs for the CI run",
            Self::LogJob { .. } => "read the CI job log",
            Self::WaitMr { .. } => "wait for the merge request to change",
            Self::WaitRun { .. } => "wait for the CI run to finish",
            Self::WaitJob { .. } => "wait for the CI job to finish",
            Self::OpenMr { .. } => "open the merge request",
            Self::SetMr { .. } => "set the merge request state",
            Self::EditMr { .. } => "edit the merge request",
            Self::CommentMr { .. } => "comment on the merge request",
            Self::ReplyThread { .. } => "reply to the review thread",
            Self::SetThread { .. } => "set the review thread state",
            Self::RetryJob { .. } => "retry the CI job",
            Self::CancelJob { .. } => "cancel the CI job",
            Self::CancelRun { .. } => "cancel the CI run",
        }
    }

    fn resource(&self) -> String {
        match self {
            Self::ShowMr { mr }
            | Self::ListThreads { mr }
            | Self::WaitMr { mr }
            | Self::SetMr { mr, .. }
            | Self::EditMr { mr, .. }
            | Self::CommentMr { mr, .. } => format!("mr {mr}"),
            Self::ReplyThread { mr, thread, .. } | Self::SetThread { mr, thread, .. } => {
                format!("thread {thread} in mr {mr}")
            }
            Self::ShowRun { run }
            | Self::ListJobs { run }
            | Self::WaitRun { run }
            | Self::CancelRun { run } => format!("run {run}"),
            Self::ShowJob { job }
            | Self::LogJob { job }
            | Self::WaitJob { job }
            | Self::RetryJob { job }
            | Self::CancelJob { job } => format!("job {job}"),
            Self::ListCi { commit } => format!("commit {commit}"),
            Self::OpenMr { head, base, .. } => format!("mr {head} -> {base}"),
        }
    }
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

pub(crate) fn render_cli_command_output(output: ProviderCommandOutput) -> String {
    match output {
        ProviderCommandOutput::ChangeRequest(request) => render_change_request(&request),
        ProviderCommandOutput::ReviewThreads(threads) => render_threads(&threads),
        ProviderCommandOutput::CiJobs(jobs) => render_jobs(&jobs),
        ProviderCommandOutput::CiRun(run) => render_run(&run),
        ProviderCommandOutput::CiRunsAndJobs { runs, jobs } => render_ci(&runs, &jobs),
        ProviderCommandOutput::CiJob(job) => render_job(&job),
        ProviderCommandOutput::CiJobLog(log) => tail_ci_job_log(log),
        ProviderCommandOutput::Confirmation(message) => format!("{message}\n"),
        ProviderCommandOutput::CurrentStatus(_) => {
            unreachable!("contextual status is not a public CLI result")
        }
    }
}

fn render_change_request(request: &ChangeRequestStatus) -> String {
    format!(
        "MR: {}\nState: {}{}\nTitle: {}\nHead: {}\nBase: {}\nURL: {}\n",
        request.handle,
        request.state,
        if request.draft { " (draft)" } else { "" },
        request.title,
        request.head,
        request.base,
        request.url
    )
}

fn render_run(run: &CiRun) -> String {
    format!(
        "Run: {}\nState: {}\nName: {}\nCommit: {}\nRef: {}\nURL: {}\n",
        run.handle,
        run.state,
        run.name,
        run.head,
        run.branch.as_deref().unwrap_or("unknown"),
        run.url.as_deref().unwrap_or("unavailable")
    )
}

fn render_job(job: &CiJob) -> String {
    format!(
        "Job: {}\nRun: {}\nState: {}\nName: {}\nURL: {}\n",
        job.handle,
        job.run.as_deref().unwrap_or("unknown"),
        job.state,
        job.name,
        job.url.as_deref().unwrap_or("unavailable")
    )
}

fn render_ci(runs: &[CiRun], jobs: &[CiJob]) -> String {
    let mut output = runs
        .iter()
        .map(|run| format!("run {} [{}] {}\n", run.handle, run.state, run.name))
        .collect::<String>();
    output.push_str(
        &jobs
            .iter()
            .map(|job| {
                format!(
                    "job {} run {} [{}] {}\n",
                    job.handle,
                    job.run.as_deref().unwrap_or("unknown"),
                    job.state,
                    job.name
                )
            })
            .collect::<String>(),
    );
    if output.is_empty() {
        "No CI resources for the commit.\n".to_owned()
    } else {
        output
    }
}

fn tail_ci_job_log(log: String) -> String {
    tail_ci_job_log_at_limit(log, CI_JOB_LOG_TAIL_LIMIT)
}

fn tail_ci_job_log_at_limit(log: String, limit: usize) -> String {
    if log.len() <= limit {
        return log;
    }
    let retained = limit - CI_JOB_LOG_TRUNCATION_NOTICE.len();
    let mut start = log.len() - retained;
    while !log.is_char_boundary(start) {
        start += 1;
    }
    format!("{CI_JOB_LOG_TRUNCATION_NOTICE}{}", &log[start..])
}

fn render_threads(threads: &[ReviewThread]) -> String {
    if threads.is_empty() {
        return "No review threads.\n".to_owned();
    }
    let mut output = String::new();
    for thread in threads {
        let location = match (&thread.path, thread.line) {
            (Some(path), Some(line)) => format!(" {path}:{line}"),
            (Some(path), None) => format!(" {path}"),
            _ => String::new(),
        };
        output.push_str(&format!(
            "{} [{}]{}\n",
            thread.handle,
            if !thread.resolvable {
                "feedback"
            } else if thread.resolved {
                "resolved"
            } else {
                "open"
            },
            location
        ));
        for comment in &thread.comments {
            output.push_str(&format!("  {}: {}\n", comment.author, comment.body));
            if let Some(url) = &comment.url {
                output.push_str(&format!("  {url}\n"));
            }
        }
    }
    output
}

fn render_jobs(jobs: &[CiJob]) -> String {
    if jobs.is_empty() {
        return "No CI jobs for the current commit.\n".to_owned();
    }
    let mut output = jobs
        .iter()
        .map(|job| {
            let url = job
                .url
                .as_deref()
                .map(|url| format!("\n  {url}"))
                .unwrap_or_default();
            format!("{} [{}] {}{url}\n", job.handle, job.state, job.name)
        })
        .collect::<String>();
    output.push_str("\nUse `ag-git show job ID`, `ag-git log job ID`, or `ag-git wait job ID`.\n");
    output
}

fn required_text(args: &[String], usage: &str) -> Result<String> {
    if args.is_empty() {
        bail!("usage: {usage}");
    }
    Ok(args.join(" "))
}

fn numeric(value: &str, kind: &str) -> Result<u64> {
    value
        .parse()
        .map_err(|_| anyhow::anyhow!("{kind} ID must be a positive integer"))
        .and_then(|id| {
            if id == 0 {
                bail!("{kind} ID must be a positive integer")
            } else {
                Ok(id)
            }
        })
}

fn validate_commit(commit: &str) -> Result<()> {
    if commit.len() < 7 || commit.len() > 64 || !commit.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        bail!("commit must be a 7 to 64 character hexadecimal object ID");
    }
    Ok(())
}

fn parse_open_mr(args: &[String]) -> Result<CliCommand> {
    let [kind, options @ ..] = args else {
        bail!("usage: ag-git open mr --head BRANCH --base BRANCH [--draft]");
    };
    if kind != "mr" {
        bail!("usage: ag-git open mr --head BRANCH --base BRANCH [--draft]");
    }
    let mut head = None;
    let mut base = None;
    let mut draft = false;
    let mut index = 0;
    while index < options.len() {
        match options[index].as_str() {
            "--head" | "--base" => {
                let target = if options[index] == "--head" {
                    &mut head
                } else {
                    &mut base
                };
                index += 1;
                let value = options
                    .get(index)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "usage: ag-git open mr --head BRANCH --base BRANCH [--draft]"
                        )
                    })?;
                if target.replace(value.clone()).is_some() {
                    bail!("open mr option specified more than once");
                }
            }
            "--draft" if !draft => draft = true,
            _ => bail!("usage: ag-git open mr --head BRANCH --base BRANCH [--draft]"),
        }
        index += 1;
    }
    Ok(CliCommand::OpenMr {
        head: head.context("open mr requires --head BRANCH")?,
        base: base.context("open mr requires --base BRANCH")?,
        draft,
    })
}

fn parse_set(args: &[String]) -> Result<CliCommand> {
    match args {
        [kind, id, state] if kind == "mr" => {
            let state = match state.as_str() {
                "ready" => ChangeRequestState::Ready,
                "draft" => ChangeRequestState::Draft,
                "open" => ChangeRequestState::Open,
                "closed" => ChangeRequestState::Closed,
                _ => bail!("MR state must be ready, draft, open, or closed"),
            };
            Ok(CliCommand::SetMr { mr: numeric(id, "MR")?, state })
        }
        [kind, thread, flag, mr, state] if kind == "thread" && flag == "--mr" => {
            let resolved = match state.as_str() {
                "resolved" => true,
                "open" => false,
                _ => bail!("thread state must be resolved or open"),
            };
            Ok(CliCommand::SetThread {
                mr: numeric(mr, "MR")?,
                thread: ReviewThreadHandle::new(thread),
                resolved,
            })
        }
        _ => bail!("usage: ag-git set mr ID ready|draft|open|closed | set thread ID --mr MR_ID resolved|open"),
    }
}

fn parse_explicit_edit(args: &[String]) -> Result<CliCommand> {
    let [kind, id, options @ ..] = args else {
        bail!("usage: ag-git edit mr ID [--title TEXT] [--body TEXT]");
    };
    if kind != "mr" {
        bail!("usage: ag-git edit mr ID [--title TEXT] [--body TEXT]");
    }
    let mut title = None;
    let mut body = None;
    let mut index = 0;
    while index < options.len() {
        let target = match options[index].as_str() {
            "--title" => &mut title,
            "--body" => &mut body,
            _ => bail!("usage: ag-git edit mr ID [--title TEXT] [--body TEXT]"),
        };
        index += 1;
        let value = options
            .get(index)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                anyhow::anyhow!("usage: ag-git edit mr ID [--title TEXT] [--body TEXT]")
            })?;
        if target.replace(value.clone()).is_some() {
            bail!("edit option specified more than once");
        }
        index += 1;
    }
    if title.is_none() && body.is_none() {
        bail!("edit requires --title or --body");
    }
    Ok(CliCommand::EditMr {
        mr: numeric(id, "MR")?,
        title,
        body,
    })
}

fn parse_reply(args: &[String]) -> Result<CliCommand> {
    let [kind, thread, flag, mr, body @ ..] = args else {
        bail!("usage: ag-git reply thread ID --mr MR_ID TEXT");
    };
    if kind != "thread" || flag != "--mr" {
        bail!("usage: ag-git reply thread ID --mr MR_ID TEXT");
    }
    Ok(CliCommand::ReplyThread {
        mr: numeric(mr, "MR")?,
        thread: ReviewThreadHandle::new(thread),
        body: required_text(body, "ag-git reply thread ID --mr MR_ID TEXT")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ci_job_logs_keep_only_a_bounded_tail() {
        let output = tail_ci_job_log_at_limit(
            "012345678901234567890123456789αβγδεζηθικλμνξ".to_owned(),
            48,
        );

        insta::assert_snapshot!(output, @r###"
        [earlier CI log output omitted]
        ηθικλμνξ
        "###);
    }

    #[test]
    fn command_parser_rejects_ambiguous_inputs() {
        assert_eq!(
            CliCommand::parse(&["wait".into(), "job".into(), "42".into()]).unwrap(),
            CliCommand::WaitJob { job: 42 }
        );
        assert_eq!(
            CliCommand::parse(&[
                "open".into(),
                "mr".into(),
                "--head".into(),
                "wt/fix".into(),
                "--base".into(),
                "main".into(),
                "--draft".into(),
            ])
            .unwrap(),
            CliCommand::OpenMr {
                head: "wt/fix".to_owned(),
                base: "main".to_owned(),
                draft: true,
            }
        );
        assert!(CliCommand::parse(&[]).is_err());
        assert!(CliCommand::parse(&["wait".into()]).is_err());
        assert!(CliCommand::parse(&["log".into(), "42".into()]).is_err());
        assert!(CliCommand::parse(&["open-mr".into()]).is_err());
    }

    #[test]
    fn command_errors_include_complete_agent_context() {
        let scope = ProviderCommandScope {
            host: "github.example",
            project: "acme/widget",
            base: "main",
            prefix: "df1/",
            branch: "df1/fix-login",
            head: "abc123",
        };
        let error = with_provider_command_context(
            Err(anyhow::anyhow!(
                "review thread `T9` was not found; run `ag-git list threads mr ID` and use its provider ID"
            )),
            ProviderKind::GitHub,
            &scope,
            &ProviderCommand::SetReviewThreadResolved {
                thread: ReviewThreadHandle::new("T9"),
                resolved: true,
            },
        )
        .unwrap_err();

        insta::assert_snapshot!(format!("{error:#}"), @r###"
        ag-git could not resolve the review thread
        Provider: GitHub (github.example)
        Project: acme/widget
        Branch: df1/fix-login
        Base: main
        Current commit: abc123
        Cause: review thread `T9` was not found; run `ag-git list threads mr ID` and use its provider ID
        "###);
    }

    #[test]
    fn review_output_includes_actionable_commands() {
        let threads = vec![ReviewThread {
            handle: ReviewThreadHandle::new("T:thread-1"),
            resolvable: true,
            resolved: false,
            path: Some("src/login.rs".to_owned()),
            line: Some(42),
            comments: vec![ReviewComment {
                author: "reviewer".to_owned(),
                body: "Handle this error.".to_owned(),
                url: Some("https://github.test/thread-1".to_owned()),
            }],
        }];

        insta::assert_snapshot!(render_threads(&threads), @r###"
        T:thread-1 [open] src/login.rs:42
          reviewer: Handle this error.
          https://github.test/thread-1
        "###);
    }
}
