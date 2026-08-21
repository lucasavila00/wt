mod client;

use super::{
    ChangeRequestState, ChangeRequestStatus, CiJob, CiJobHandle, CiRun, CliCommand, GitProviderApi,
    ProviderCommand, ProviderCommandOutput, ProviderCommandScope, ProviderProjectScope,
    ReviewComment, ReviewThread, ReviewThreadHandle,
};
use crate::api::http::{ProviderAuthentication, ProviderHttpClient};
use anyhow::{bail, Context, Result};
use graphql_client::GraphQLQuery;
use serde::{Deserialize, Serialize};

// graphql_client resolves custom scalars by their schema names. Transparent
// newtypes keep those JSON values typed without changing their wire format.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(transparent)]
struct URI(String);

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(transparent)]
struct GitObjectID(String);

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/github/schema.graphql",
    query_path = "graphql/github/request.graphql",
    response_derives = "Debug"
)]
struct GithubReadPullRequest;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/github/schema.graphql",
    query_path = "graphql/github/pull_request_by_number.graphql",
    response_derives = "Debug"
)]
struct GithubReadPullRequestByNumber;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/github/schema.graphql",
    query_path = "graphql/github/create.graphql",
    response_derives = "Debug"
)]
struct GithubCreatePullRequest;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/github/schema.graphql",
    query_path = "graphql/github/update.graphql",
    operation_name = "GithubUpdatePullRequest",
    response_derives = "Debug"
)]
struct GithubUpdatePullRequest;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/github/schema.graphql",
    query_path = "graphql/github/update.graphql",
    operation_name = "GithubMarkPullRequestReady",
    response_derives = "Debug"
)]
struct GithubMarkPullRequestReady;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/github/schema.graphql",
    query_path = "graphql/github/update.graphql",
    operation_name = "GithubMarkPullRequestDraft",
    response_derives = "Debug"
)]
struct GithubMarkPullRequestDraft;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/github/schema.graphql",
    query_path = "graphql/github/update.graphql",
    operation_name = "GithubAddPullRequestComment",
    response_derives = "Debug"
)]
struct GithubAddPullRequestComment;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/github/schema.graphql",
    query_path = "graphql/github/update.graphql",
    operation_name = "GithubReplyToReviewThread",
    response_derives = "Debug"
)]
struct GithubReplyToReviewThread;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/github/schema.graphql",
    query_path = "graphql/github/update.graphql",
    operation_name = "GithubResolveReviewThread",
    response_derives = "Debug"
)]
struct GithubResolveReviewThread;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/github/schema.graphql",
    query_path = "graphql/github/update.graphql",
    operation_name = "GithubReopenReviewThread",
    response_derives = "Debug"
)]
struct GithubReopenReviewThread;

pub struct GithubApi {
    graphql: ProviderHttpClient,
    rest: ProviderHttpClient,
    graphql_path: &'static str,
    rest_prefix: &'static str,
}

#[derive(Debug)]
struct GithubChangeRequestSnapshot {
    repository_id: GithubRepositoryId,
    pull_request_id: Option<GithubPullRequestId>,
    request: Option<ChangeRequestStatus>,
    review_targets: Vec<(String, GithubReviewTarget)>,
    jobs: Vec<CiJob>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum GithubReviewTarget {
    Thread(GithubReviewThreadId),
    PullRequest(GithubPullRequestId),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
struct GithubRepositoryId(String);

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
struct GithubPullRequestId(String);

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
struct GithubReviewThreadId(String);

#[derive(Deserialize)]
struct WorkflowRuns {
    total_count: u64,
    workflow_runs: Vec<WorkflowRun>,
}

#[derive(Clone, Deserialize, Eq, PartialEq)]
struct WorkflowRun {
    id: u64,
    #[serde(default)]
    name: String,
    #[serde(default, rename = "event")]
    trigger: Option<String>,
    #[serde(default)]
    status: String,
    conclusion: Option<String>,
    html_url: Option<String>,
    #[serde(default)]
    head_sha: String,
    head_branch: Option<String>,
    head_repository: Option<WorkflowRunRepository>,
}

#[derive(Clone, Deserialize, Eq, PartialEq)]
struct WorkflowRunRepository {
    full_name: String,
}

#[derive(Deserialize)]
struct WorkflowJobs {
    total_count: u64,
    jobs: Vec<WorkflowJob>,
}

#[derive(Clone, Deserialize, Eq, PartialEq)]
struct WorkflowJob {
    id: u64,
    name: String,
    status: String,
    conclusion: Option<String>,
    html_url: Option<String>,
    #[serde(default)]
    run_id: u64,
}

#[derive(Deserialize)]
struct CheckRunAnnotation {
    path: String,
    start_line: u64,
    end_line: u64,
    annotation_level: String,
    title: Option<String>,
    message: String,
    raw_details: Option<String>,
}

#[derive(Clone, Deserialize, Eq, PartialEq)]
struct PullRequest {
    number: u64,
    node_id: String,
    html_url: String,
    title: String,
    #[serde(default)]
    body: Option<String>,
    state: String,
    draft: bool,
    head: PullRequestRef,
    base: PullRequestRef,
}

#[derive(Clone, Deserialize, Eq, PartialEq)]
struct PullRequestRef {
    #[serde(rename = "ref")]
    reference: String,
    sha: String,
    repo: Option<PullRequestRepository>,
}

#[derive(Clone, Deserialize, Eq, PartialEq)]
struct PullRequestRepository {
    full_name: String,
}

#[derive(Deserialize)]
struct Commit {
    sha: String,
}

fn pull_request_status(request: PullRequest) -> ChangeRequestStatus {
    ChangeRequestStatus {
        handle: request.number.to_string(),
        url: request.html_url,
        title: request.title,
        body: request.body,
        state: request.state,
        draft: request.draft,
        head: request.head.sha,
        base: request.base.reference,
        review_state: None,
        threads: Vec::new(),
        jobs: Vec::new(),
    }
}

fn github_job_log_pending(state: &str) -> bool {
    matches!(
        state,
        "queued" | "in_progress" | "waiting" | "pending" | "requested"
    )
}

fn ci_job(job: WorkflowJob) -> CiJob {
    CiJob {
        handle: CiJobHandle::new(job.id.to_string()),
        run: (job.run_id != 0).then(|| job.run_id.to_string()),
        name: job.name,
        state: job.conclusion.unwrap_or(job.status),
        url: job.html_url,
    }
}

fn ci_run(run: WorkflowRun) -> CiRun {
    CiRun {
        handle: run.id.to_string(),
        name: if run.name.is_empty() {
            "workflow run".to_owned()
        } else {
            run.name
        },
        state: run.conclusion.unwrap_or(run.status),
        trigger: run.trigger,
        url: run.html_url,
        head: run.head_sha,
        branch: run.head_branch,
    }
}

fn ci_terminal(state: &str) -> bool {
    !matches!(
        state,
        "queued" | "in_progress" | "waiting" | "pending" | "requested"
    )
}

fn split_project(project: &str) -> Result<(&str, &str)> {
    let (owner, name) = project
        .split_once('/')
        .ok_or_else(|| anyhow::anyhow!("GitHub project must be OWNER/REPOSITORY"))?;
    if owner.is_empty() || name.is_empty() || name.contains('/') {
        bail!("GitHub project must be OWNER/REPOSITORY");
    }
    Ok((owner, name))
}

fn ensure_complete_connection(name: &str, has_next_page: bool, total_count: i64) -> Result<()> {
    if has_next_page {
        bail!(
            "GitHub returned only the first page of {name} ({total_count} total); wt-tools refuses to continue with incomplete handles or status"
        );
    }
    Ok(())
}

fn ensure_complete_rest_collection(name: &str, total_count: u64, received: usize) -> Result<()> {
    if total_count != received as u64 {
        bail!(
            "GitHub returned only {received} of {total_count} {name}; wt-tools refuses to continue with incomplete CI handles or status"
        );
    }
    Ok(())
}

fn title_from_branch(scope: &ProviderCommandScope<'_>) -> String {
    scope
        .branch
        .strip_prefix(scope.prefix)
        .unwrap_or(scope.branch)
        .replace(['-', '_'], " ")
}

#[cfg(test)]
mod tests;
