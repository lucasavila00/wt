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
struct NoteableID(String);

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(transparent)]
struct NoteID(String);

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(transparent)]
struct DiscussionID(String);

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/gitlab/schema.json",
    query_path = "graphql/gitlab/request.graphql",
    response_derives = "Debug"
)]
struct GitlabReadMergeRequest;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/gitlab/schema.json",
    query_path = "graphql/gitlab/merge_request_by_iid.graphql",
    response_derives = "Debug"
)]
struct GitlabReadMergeRequestByIid;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/gitlab/schema.json",
    query_path = "graphql/gitlab/create.graphql",
    response_derives = "Debug"
)]
struct GitlabCreateMergeRequest;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/gitlab/schema.json",
    query_path = "graphql/gitlab/update.graphql",
    operation_name = "GitlabUpdateMergeRequest",
    response_derives = "Debug"
)]
struct GitlabUpdateMergeRequest;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/gitlab/schema.json",
    query_path = "graphql/gitlab/update.graphql",
    operation_name = "GitlabSetMergeRequestDraft",
    response_derives = "Debug"
)]
struct GitlabSetMergeRequestDraft;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/gitlab/schema.json",
    query_path = "graphql/gitlab/update.graphql",
    operation_name = "GitlabAddMergeRequestComment",
    response_derives = "Debug"
)]
struct GitlabAddMergeRequestComment;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/gitlab/schema.json",
    query_path = "graphql/gitlab/update.graphql",
    operation_name = "GitlabReplyToDiscussion",
    response_derives = "Debug"
)]
struct GitlabReplyToDiscussion;

#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "graphql/gitlab/schema.json",
    query_path = "graphql/gitlab/update.graphql",
    operation_name = "GitlabSetDiscussionResolved",
    response_derives = "Debug"
)]
struct GitlabSetDiscussionResolved;

pub(crate) struct GitlabApi {
    http: ProviderHttpClient,
}

struct GitlabChangeRequestSnapshot {
    merge_request_id: Option<GitlabMergeRequestId>,
    merge_request_number: Option<GitlabMergeRequestNumber>,
    request: Option<ChangeRequestStatus>,
    discussions: Vec<(ReviewThreadHandle, GitlabDiscussionId)>,
}

struct GitlabDirectMergeRequest {
    id: GitlabMergeRequestId,
    head: Option<String>,
    threads: Vec<ReviewThread>,
    discussions: Vec<(ReviewThreadHandle, GitlabDiscussionId)>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
struct GitlabMergeRequestId(String);

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
struct GitlabMergeRequestNumber(String);

impl std::fmt::Display for GitlabMergeRequestNumber {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
struct GitlabDiscussionId(String);

#[derive(Clone, Deserialize, Eq, PartialEq)]
struct Pipeline {
    id: u64,
    status: String,
    #[serde(default, rename = "source")]
    trigger: Option<String>,
    web_url: Option<String>,
    yaml_errors: Option<String>,
    #[serde(default)]
    sha: String,
    #[serde(default, rename = "ref")]
    reference: Option<String>,
}

#[derive(Deserialize)]
struct MergeRequestDetails {
    sha: String,
    head_pipeline: Option<Pipeline>,
}

#[derive(Clone, Deserialize, Eq, PartialEq)]
struct PipelineJob {
    id: u64,
    name: String,
    status: String,
    web_url: Option<String>,
    #[serde(default, rename = "ref")]
    reference: Option<String>,
    #[serde(default)]
    pipeline: Option<JobPipeline>,
}

#[derive(Clone, Deserialize, Eq, PartialEq)]
struct JobPipeline {
    id: u64,
}

#[derive(Clone, Deserialize, Eq, PartialEq)]
struct MergeRequest {
    iid: u64,
    title: String,
    #[serde(default)]
    description: Option<String>,
    web_url: String,
    state: String,
    #[serde(default)]
    draft: bool,
    sha: String,
    source_branch: String,
    target_branch: String,
    source_project_id: Option<u64>,
    target_project_id: Option<u64>,
}

#[derive(Deserialize)]
struct Commit {
    id: String,
}

fn set_change_request_draft(
    http: &ProviderHttpClient,
    project: &str,
    merge_request_number: GitlabMergeRequestNumber,
    draft: bool,
) -> Result<()> {
    let data = http.execute_graphql::<GitlabSetMergeRequestDraft>(
        "api/graphql",
        gitlab_set_merge_request_draft::Variables {
            project: project.to_owned(),
            iid: merge_request_number.0,
            draft,
        },
    )?;
    ensure_errors(
        data.merge_request_set_draft
            .context("GitLab returned no result")?
            .errors,
    )
}

fn ensure_errors(errors: Vec<String>) -> Result<()> {
    if errors.is_empty() {
        Ok(())
    } else {
        bail!("GitLab API error: {}", errors.join("; "))
    }
}

fn review_thread_handle(id: &GitlabDiscussionId) -> ReviewThreadHandle {
    ReviewThreadHandle::new(id.0.clone())
}

fn normalized_ci_state(state: String) -> String {
    if state == "canceled" {
        "cancelled".to_owned()
    } else {
        state
    }
}

fn gitlab_job(job: PipelineJob) -> CiJob {
    CiJob {
        handle: CiJobHandle::new(job.id.to_string()),
        run: job.pipeline.map(|pipeline| pipeline.id.to_string()),
        name: job.name,
        state: normalized_ci_state(job.status),
        url: job.web_url,
    }
}

fn gitlab_run(pipeline: Pipeline) -> CiRun {
    CiRun {
        handle: pipeline.id.to_string(),
        name: "pipeline".to_owned(),
        state: normalized_ci_state(pipeline.status),
        trigger: pipeline.trigger,
        url: pipeline.web_url,
        head: pipeline.sha,
        branch: pipeline.reference,
    }
}

fn gitlab_ci_terminal(state: &str) -> bool {
    !matches!(
        state,
        "created" | "waiting_for_resource" | "preparing" | "pending" | "running" | "scheduled"
    )
}

fn merge_request_status(request: MergeRequest) -> ChangeRequestStatus {
    ChangeRequestStatus {
        handle: request.iid.to_string(),
        url: request.web_url,
        title: request.title,
        body: request.description,
        state: request.state,
        draft: request.draft,
        head: request.sha,
        base: request.target_branch,
        review_state: None,
        threads: Vec::new(),
        jobs: Vec::new(),
    }
}

fn encoded_project(project: &str) -> String {
    url::form_urlencoded::byte_serialize(project.as_bytes()).collect()
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
