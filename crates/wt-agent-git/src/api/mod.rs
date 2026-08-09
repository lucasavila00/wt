mod github;
mod gitlab;
mod http;

use crate::ProviderKind;
use anyhow::{bail, Result};
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

#[derive(Clone, Debug, Eq, PartialEq)]
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CiJobHandle(String);

impl CiJobHandle {
    pub(crate) fn new(value: impl Into<String>) -> Self {
        Self(value.into())
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
    fn execute_command(
        &self,
        scope: &ProviderCommandScope<'_>,
        command: &ProviderCommand,
    ) -> Result<ProviderCommandOutput>;
}

pub(crate) fn execute_provider_command(
    kind: ProviderKind,
    token_file: &Path,
    scope: &ProviderCommandScope<'_>,
    command: &ProviderCommand,
) -> Result<ProviderCommandOutput> {
    let token = std::fs::read_to_string(token_file)
        .map_err(|error| anyhow::anyhow!("read provider API credential: {error}"))?;
    let token = token.trim();
    if token.is_empty() {
        bail!("provider API credential is empty");
    }
    match kind {
        ProviderKind::GitHub => {
            github::GithubApi::new(scope.host, token)?.execute_command(scope, command)
        }
        ProviderKind::GitLab => {
            gitlab::GitlabApi::new(scope.host, token)?.execute_command(scope, command)
        }
    }
}

impl ProviderCommand {
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
        .filter(|thread| !thread.resolved)
        .count();
    output.push_str(&format!("Unresolved threads: {unresolved}\n"));
    let failing = request
        .jobs
        .iter()
        .filter(|job| matches!(job.state.as_str(), "failed" | "failure" | "cancelled"))
        .count();
    output.push_str(&format!(
        "CI jobs: {} ({failing} failing)\n",
        request.jobs.len()
    ));
    output
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
            if thread.resolved { "resolved" } else { "open" },
            location
        ));
        for comment in &thread.comments {
            output.push_str(&format!("  {}: {}\n", comment.author, comment.body));
        }
    }
    output
}

fn render_jobs(jobs: &[CiJob]) -> String {
    if jobs.is_empty() {
        return "No CI jobs for the current commit.\n".to_owned();
    }
    jobs.iter()
        .map(|job| format!("{} [{}] {}\n", job.handle, job.state, job.name))
        .collect()
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
}
