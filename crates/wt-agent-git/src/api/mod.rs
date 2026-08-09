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

pub(crate) struct ProviderCommandScope<'a> {
    pub host: &'a str,
    pub project: &'a str,
    pub base: &'a str,
    pub prefix: &'a str,
    pub branch: &'a str,
    pub head: &'a str,
}

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
    pub name: String,
    pub state: String,
    pub url: Option<String>,
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

    pub(crate) fn parse(args: &[String]) -> Result<Self> {
        let Some((name, rest)) = args.split_first() else {
            return Ok(Self::ReadCurrentStatus);
        };
        match name.as_str() {
            "open-mr" => match rest {
                [] => Ok(Self::OpenChangeRequest { draft: false }),
                [flag] if flag == "--draft" => Ok(Self::OpenChangeRequest { draft: true }),
                _ => bail!("usage: ag-git open-mr [--draft]"),
            },
            "ready" => no_args(rest, Self::MarkChangeRequestReady, "ag-git ready"),
            "draft" => no_args(rest, Self::MarkChangeRequestDraft, "ag-git draft"),
            "comment" => Ok(Self::AddChangeRequestComment {
                body: required_text(rest, "ag-git comment TEXT")?,
            }),
            "edit" => parse_edit(rest),
            "review" => no_args(rest, Self::ReadReviewThreads, "ag-git review"),
            "reply" => {
                let (thread, body) = split_handle_text(rest, "ag-git reply HANDLE TEXT")?;
                Ok(Self::ReplyToReviewThread {
                    thread: ReviewThreadHandle::new(thread),
                    body,
                })
            }
            "resolve" => Ok(Self::SetReviewThreadResolved {
                thread: ReviewThreadHandle::new(one_arg(rest, "ag-git resolve HANDLE")?),
                resolved: true,
            }),
            "reopen" => Ok(Self::SetReviewThreadResolved {
                thread: ReviewThreadHandle::new(one_arg(rest, "ag-git reopen HANDLE")?),
                resolved: false,
            }),
            "ci" => no_args(rest, Self::ReadCiJobs, "ag-git ci"),
            "log" => Ok(Self::ReadCiJobLog {
                job: CiJobHandle::new(one_arg(rest, "ag-git log JOB")?),
            }),
            "retry" => Ok(Self::RetryCiJob {
                job: CiJobHandle::new(one_arg(rest, "ag-git retry JOB")?),
            }),
            "cancel" => Ok(Self::CancelCiJob {
                job: CiJobHandle::new(one_arg(rest, "ag-git cancel JOB")?),
            }),
            "wait" => no_args(rest, Self::WaitForReviewOrCiChange, "ag-git wait"),
            "close" => no_args(rest, Self::CloseChangeRequest, "ag-git close"),
            "reopen-mr" => no_args(rest, Self::ReopenChangeRequest, "ag-git reopen-mr"),
            _ => bail!("unknown command `{name}`; run `ag-git --help`"),
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

pub(crate) fn render_provider_command_output(
    output: ProviderCommandOutput,
    scope: &ProviderCommandScope<'_>,
) -> String {
    match output {
        ProviderCommandOutput::CurrentStatus(request) => render_status(request.as_ref(), scope),
        ProviderCommandOutput::ChangeRequest(request) => {
            format!("{}: {}\n{}\n", request.handle, request.title, request.url)
        }
        ProviderCommandOutput::ReviewThreads(threads) => render_threads(&threads),
        ProviderCommandOutput::CiJobs(jobs) => render_jobs(&jobs),
        ProviderCommandOutput::CiJobLog(log) => log,
        ProviderCommandOutput::Confirmation(message) => format!("{message}\n"),
    }
}

fn render_status(
    request: Option<&ChangeRequestStatus>,
    scope: &ProviderCommandScope<'_>,
) -> String {
    let mut output = format!(
        "Project: {}\nBranch: {} ({})\nBase: {}\n",
        scope.project, scope.branch, scope.head, scope.base
    );
    let Some(request) = request else {
        output.push_str("Request: none\n\nOpen one with `ag-git open-mr`.\n");
        return output;
    };
    output.push_str(&format!(
        "Request: {} {} [{}{}]\nURL: {}\n",
        request.handle,
        request.title,
        request.state,
        if request.draft { ", draft" } else { "" },
        request.url
    ));
    if let Some(review) = &request.review_state {
        output.push_str(&format!("Review: {review}\n"));
    }
    let unresolved = request
        .threads
        .iter()
        .filter(|thread| thread.resolvable && !thread.resolved)
        .count();
    output.push_str(&format!("Unresolved threads: {unresolved}\n"));
    let failing = request
        .jobs
        .iter()
        .filter(|job| ci_failed(&job.state))
        .count();
    let passing = request
        .jobs
        .iter()
        .filter(|job| ci_passed(&job.state))
        .count();
    let pending = request.jobs.len() - failing - passing;
    let ci = if request.jobs.is_empty() {
        "no jobs".to_owned()
    } else if failing > 0 {
        format!("failing ({failing} failed, {pending} pending)")
    } else if pending > 0 {
        format!("pending ({pending} not finished)")
    } else {
        format!("passing ({passing} passed)")
    };
    output.push_str(&format!("CI: {ci}\n"));
    if request.draft {
        output.push_str("\nMark it ready with `ag-git ready`.\n");
    }
    if unresolved > 0 || request.review_state.as_deref() == Some("changes_requested") {
        output.push_str("Read and answer review feedback with `ag-git review`.\n");
    }
    if failing > 0 {
        output.push_str("Inspect failures with `ag-git ci`, then `ag-git log JOB`.\n");
    }
    output
}

fn ci_failed(state: &str) -> bool {
    matches!(
        state,
        "failed"
            | "failure"
            | "cancelled"
            | "canceled"
            | "timed_out"
            | "action_required"
            | "startup_failure"
    )
}

fn ci_passed(state: &str) -> bool {
    matches!(state, "success" | "passed" | "skipped" | "neutral")
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
    output.push_str("\nReply with `ag-git reply HANDLE TEXT`.\n");
    if threads
        .iter()
        .any(|thread| thread.resolvable && !thread.resolved)
    {
        output.push_str("Resolve addressed feedback with `ag-git resolve HANDLE`.\n");
    }
    if threads
        .iter()
        .any(|thread| thread.resolvable && thread.resolved)
    {
        output.push_str("Reopen feedback with `ag-git reopen HANDLE`.\n");
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
    output.push_str(
        "\nInspect a job with `ag-git log JOB`. Retry or cancel it with `ag-git retry JOB` or `ag-git cancel JOB` when the provider allows it.\n",
    );
    output
}

fn no_args(rest: &[String], command: ProviderCommand, usage: &str) -> Result<ProviderCommand> {
    if rest.is_empty() {
        Ok(command)
    } else {
        bail!("usage: {usage}")
    }
}

fn required_text(args: &[String], usage: &str) -> Result<String> {
    if args.is_empty() {
        bail!("usage: {usage}");
    }
    Ok(args.join(" "))
}

fn one_arg(args: &[String], usage: &str) -> Result<String> {
    match args {
        [value] => Ok(value.clone()),
        _ => bail!("usage: {usage}"),
    }
}

fn split_handle_text(args: &[String], usage: &str) -> Result<(String, String)> {
    let Some((handle, body)) = args.split_first() else {
        bail!("usage: {usage}");
    };
    Ok((handle.clone(), required_text(body, usage)?))
}

fn parse_edit(args: &[String]) -> Result<ProviderCommand> {
    let mut title = None;
    let mut body = None;
    let mut index = 0;
    while index < args.len() {
        let target = match args[index].as_str() {
            "--title" => &mut title,
            "--body" => &mut body,
            _ => bail!("usage: ag-git edit [--title TEXT] [--body TEXT]"),
        };
        index += 1;
        let value = args
            .get(index)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow::anyhow!("usage: ag-git edit [--title TEXT] [--body TEXT]"))?;
        if target.replace(value.clone()).is_some() {
            bail!("edit option specified more than once");
        }
        index += 1;
    }
    if title.is_none() && body.is_none() {
        bail!("edit requires --title or --body");
    }
    Ok(ProviderCommand::EditChangeRequest { title, body })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_parser_rejects_ambiguous_inputs() {
        assert_eq!(
            ProviderCommand::parse(&[]).unwrap(),
            ProviderCommand::ReadCurrentStatus
        );
        assert_eq!(
            ProviderCommand::parse(&["open-mr".into(), "--draft".into()]).unwrap(),
            ProviderCommand::OpenChangeRequest { draft: true }
        );
        assert!(ProviderCommand::parse(&["edit".into()]).is_err());
        assert!(ProviderCommand::parse(&["reviewers".into()]).is_err());
        assert!(ProviderCommand::parse(&["labels".into()]).is_err());
        assert!(ProviderCommand::parse(&["merge".into()]).is_err());
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
                "review thread `T9` was not found; run `ag-git review` and use a current thread handle"
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
        Cause: review thread `T9` was not found; run `ag-git review` and use a current thread handle
        "###);
    }

    #[test]
    fn status_output_tells_an_agent_what_to_do_next() {
        let scope = ProviderCommandScope {
            host: "github.com",
            project: "acme/widget",
            base: "main",
            prefix: "df1/",
            branch: "df1/fix-login",
            head: "abc123",
        };
        let request = ChangeRequestStatus {
            handle: "#7".to_owned(),
            url: "https://github.test/acme/widget/pull/7".to_owned(),
            title: "Fix login".to_owned(),
            state: "open".to_owned(),
            draft: true,
            head: "abc123".to_owned(),
            base: "main".to_owned(),
            review_state: Some("changes_requested".to_owned()),
            threads: vec![ReviewThread {
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
            }],
            jobs: vec![CiJob {
                handle: CiJobHandle::new("91"),
                name: "test".to_owned(),
                state: "failure".to_owned(),
                url: Some("https://github.test/job-91".to_owned()),
            }],
        };

        insta::assert_snapshot!(render_provider_command_output(
            ProviderCommandOutput::CurrentStatus(Some(request)),
            &scope,
        ), @r###"
        Project: acme/widget
        Branch: df1/fix-login (abc123)
        Base: main
        Request: #7 Fix login [open, draft]
        URL: https://github.test/acme/widget/pull/7
        Review: changes_requested
        Unresolved threads: 1
        CI: failing (1 failed, 0 pending)

        Mark it ready with `ag-git ready`.
        Read and answer review feedback with `ag-git review`.
        Inspect failures with `ag-git ci`, then `ag-git log JOB`.
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

        Reply with `ag-git reply HANDLE TEXT`.
        Resolve addressed feedback with `ag-git resolve HANDLE`.
        "###);
    }
}
