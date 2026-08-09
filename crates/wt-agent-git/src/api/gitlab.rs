use super::{
    ChangeRequestStatus, CiJob, CiJobHandle, GitProviderApi, ProviderCommand,
    ProviderCommandOutput, ProviderCommandScope, ReviewComment, ReviewThread, ReviewThreadHandle,
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
    discussion_ids: Vec<GitlabDiscussionId>,
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

#[derive(Deserialize)]
struct Pipeline {
    id: u64,
    sha: String,
}

#[derive(Deserialize)]
struct PipelineJob {
    id: u64,
    name: String,
    status: String,
    web_url: Option<String>,
}

impl GitlabApi {
    pub(crate) fn new(host: &str, token: &str) -> Result<Self> {
        Ok(Self {
            http: ProviderHttpClient::new(
                format!("https://{host}"),
                token,
                ProviderAuthentication::Gitlab,
            )?,
        })
    }

    fn read_change_request_snapshot(
        &self,
        scope: &ProviderCommandScope<'_>,
        include_jobs: bool,
    ) -> Result<GitlabChangeRequestSnapshot> {
        let data = self.http.execute_graphql::<GitlabReadMergeRequest>(
            "api/graphql",
            gitlab_read_merge_request::Variables {
                project: scope.project.to_owned(),
                branches: Some(vec![scope.branch.to_owned()]),
                bases: Some(vec![scope.base.to_owned()]),
            },
        )?;
        let project = data
            .project
            .with_context(|| format!("GitLab project {} was not found", scope.project))?;
        let mut nodes = project
            .merge_requests
            .context("GitLab returned no merge request connection")?
            .nodes
            .unwrap_or_default()
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        let node = nodes
            .iter()
            .position(|request| request.diff_head_sha.as_deref() == Some(scope.head))
            .map(|index| nodes.remove(index));
        if node.is_none() {
            if let Some(request) = nodes.first() {
                let provider_head = request.diff_head_sha.as_deref().unwrap_or("unknown");
                bail!(
                    "the merge request is at commit {provider_head}, but this checkout is at {}; push the branch first",
                    scope.head
                );
            }
        }
        let Some(node) = node else {
            return Ok(GitlabChangeRequestSnapshot {
                merge_request_id: None,
                merge_request_number: None,
                request: None,
                discussion_ids: Vec::new(),
            });
        };
        let mut discussion_ids = Vec::new();
        let threads = node
            .discussions
            .nodes
            .unwrap_or_default()
            .into_iter()
            .flatten()
            .enumerate()
            .map(|(index, thread)| {
                discussion_ids.push(GitlabDiscussionId(thread.id.0.clone()));
                ReviewThread {
                    handle: ReviewThreadHandle::new(format!("T{}", index + 1)),
                    resolved: thread.resolved,
                    path: None,
                    line: None,
                    comments: thread
                        .notes
                        .nodes
                        .unwrap_or_default()
                        .into_iter()
                        .flatten()
                        .map(|note| ReviewComment {
                            author: note
                                .author
                                .map(|author| author.username)
                                .unwrap_or_else(|| "unknown".to_owned()),
                            body: note.body,
                            url: note.url,
                        })
                        .collect(),
                }
            })
            .collect();
        let merge_request_number = GitlabMergeRequestNumber(node.iid);
        let jobs = if include_jobs {
            self.read_ci_jobs(scope, &merge_request_number)?
        } else {
            Vec::new()
        };
        let url = node.web_url.context("merge request has no URL")?;
        let request = ChangeRequestStatus {
            handle: format!("!{merge_request_number}"),
            url,
            title: node.title,
            state: format!("{:?}", node.state).to_ascii_lowercase(),
            draft: node.draft,
            head: node.diff_head_sha.unwrap_or_else(|| scope.head.to_owned()),
            base: node.target_branch,
            review_state: None,
            threads,
            jobs,
        };
        Ok(GitlabChangeRequestSnapshot {
            merge_request_id: Some(GitlabMergeRequestId(node.id)),
            merge_request_number: Some(merge_request_number),
            request: Some(request),
            discussion_ids,
        })
    }

    fn require_change_request(
        &self,
        scope: &ProviderCommandScope<'_>,
    ) -> Result<GitlabChangeRequestSnapshot> {
        let snapshot = self.read_change_request_snapshot(scope, false)?;
        if snapshot.request.is_none() {
            bail!("this branch has no merge request; run `ag-git open-mr`");
        }
        Ok(snapshot)
    }

    fn read_refreshed_change_request(
        &self,
        scope: &ProviderCommandScope<'_>,
    ) -> Result<ProviderCommandOutput> {
        Ok(ProviderCommandOutput::ChangeRequest(
            self.require_change_request(scope)?
                .request
                .context("merge request disappeared")?,
        ))
    }

    fn discussion_id(
        snapshot: &GitlabChangeRequestSnapshot,
        handle: &ReviewThreadHandle,
    ) -> Result<GitlabDiscussionId> {
        let index = handle
            .as_str()
            .strip_prefix('T')
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|index| *index > 0)
            .ok_or_else(|| anyhow::anyhow!("invalid review thread handle `{handle}`"))?;
        snapshot
            .discussion_ids
            .get(index - 1)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("review thread `{handle}` was not found"))
    }

    fn read_ci_jobs(
        &self,
        scope: &ProviderCommandScope<'_>,
        merge_request_number: &GitlabMergeRequestNumber,
    ) -> Result<Vec<CiJob>> {
        let project = encoded_project(scope.project);
        let pipelines: Vec<Pipeline> = self.http.read_json(&format!(
            "api/v4/projects/{project}/merge_requests/{merge_request_number}/pipelines"
        ))?;
        let mut jobs = Vec::new();
        for pipeline in pipelines
            .into_iter()
            .filter(|pipeline| pipeline.sha == scope.head)
        {
            let response: Vec<PipelineJob> = self.http.read_json(&format!(
                "api/v4/projects/{project}/pipelines/{}/jobs?include_retried=false&per_page=100",
                pipeline.id
            ))?;
            jobs.extend(response.into_iter().map(|job| CiJob {
                handle: CiJobHandle::new(job.id.to_string()),
                name: job.name,
                state: job.status,
                url: job.web_url,
            }));
        }
        Ok(jobs)
    }

    fn require_ci_job(&self, scope: &ProviderCommandScope<'_>, handle: &CiJobHandle) -> Result<()> {
        let merge_request_number = self
            .require_change_request(scope)?
            .merge_request_number
            .context("merge request has no number")?;
        if self
            .read_ci_jobs(scope, &merge_request_number)?
            .iter()
            .any(|job| job.handle.as_str() == handle.as_str())
        {
            Ok(())
        } else {
            bail!("CI job `{handle}` does not belong to the current commit")
        }
    }
}

impl GitProviderApi for GitlabApi {
    fn execute_command(
        &self,
        scope: &ProviderCommandScope<'_>,
        command: &ProviderCommand,
    ) -> Result<ProviderCommandOutput> {
        match command {
            ProviderCommand::ReadCurrentStatus => Ok(ProviderCommandOutput::CurrentStatus(
                self.read_change_request_snapshot(scope, true)?.request,
            )),
            ProviderCommand::OpenChangeRequest { draft } => {
                let snapshot = self.read_change_request_snapshot(scope, false)?;
                if let Some(request) = snapshot.request {
                    return Ok(ProviderCommandOutput::ChangeRequest(request));
                }
                let data = self.http.execute_graphql::<GitlabCreateMergeRequest>(
                    "api/graphql",
                    gitlab_create_merge_request::Variables {
                        project: scope.project.to_owned(),
                        branch: scope.branch.to_owned(),
                        base: scope.base.to_owned(),
                        title: title_from_branch(scope),
                    },
                )?;
                ensure_errors(
                    data.merge_request_create
                        .context("GitLab returned no result")?
                        .errors,
                )?;
                if *draft {
                    let snapshot = self.require_change_request(scope)?;
                    set_change_request_draft(
                        &self.http,
                        scope,
                        snapshot
                            .merge_request_number
                            .context("merge request has no IID")?,
                        true,
                    )?;
                }
                self.read_refreshed_change_request(scope)
            }
            ProviderCommand::MarkChangeRequestReady | ProviderCommand::MarkChangeRequestDraft => {
                let merge_request_number = self
                    .require_change_request(scope)?
                    .merge_request_number
                    .context("merge request has no IID")?;
                set_change_request_draft(
                    &self.http,
                    scope,
                    merge_request_number,
                    matches!(command, ProviderCommand::MarkChangeRequestDraft),
                )?;
                self.read_refreshed_change_request(scope)
            }
            ProviderCommand::AddChangeRequestComment { body } => {
                let id = self
                    .require_change_request(scope)?
                    .merge_request_id
                    .context("merge request has no ID")?;
                let data = self.http.execute_graphql::<GitlabAddMergeRequestComment>(
                    "api/graphql",
                    gitlab_add_merge_request_comment::Variables {
                        id: NoteableID(id.0),
                        body: body.clone(),
                    },
                )?;
                ensure_errors(
                    data.create_note
                        .context("GitLab returned no result")?
                        .errors,
                )?;
                Ok(ProviderCommandOutput::Confirmation(
                    "Comment added.".to_owned(),
                ))
            }
            ProviderCommand::EditChangeRequest { title, body } => {
                let merge_request_number = self
                    .require_change_request(scope)?
                    .merge_request_number
                    .context("merge request has no IID")?;
                let data = self.http.execute_graphql::<GitlabUpdateMergeRequest>(
                    "api/graphql",
                    gitlab_update_merge_request::Variables {
                        project: scope.project.to_owned(),
                        iid: merge_request_number.0,
                        title: title.clone(),
                        description: body.clone(),
                        state: None,
                    },
                )?;
                ensure_errors(
                    data.merge_request_update
                        .context("GitLab returned no result")?
                        .errors,
                )?;
                self.read_refreshed_change_request(scope)
            }
            ProviderCommand::ReadReviewThreads => Ok(ProviderCommandOutput::ReviewThreads(
                self.require_change_request(scope)?
                    .request
                    .context("merge request disappeared")?
                    .threads,
            )),
            ProviderCommand::ReplyToReviewThread { thread, body } => {
                let snapshot = self.require_change_request(scope)?;
                let discussion = Self::discussion_id(&snapshot, thread)?;
                let id = snapshot
                    .merge_request_id
                    .context("merge request has no ID")?;
                let data = self.http.execute_graphql::<GitlabReplyToDiscussion>(
                    "api/graphql",
                    gitlab_reply_to_discussion::Variables {
                        id: NoteableID(id.0),
                        discussion: DiscussionID(discussion.0),
                        body: body.clone(),
                        head: scope.head.to_owned(),
                    },
                )?;
                ensure_errors(
                    data.create_note
                        .context("GitLab returned no result")?
                        .errors,
                )?;
                Ok(ProviderCommandOutput::Confirmation(
                    "Reply added.".to_owned(),
                ))
            }
            ProviderCommand::SetReviewThreadResolved { thread, resolved } => {
                let snapshot = self.require_change_request(scope)?;
                let discussion = Self::discussion_id(&snapshot, thread)?;
                let data = self.http.execute_graphql::<GitlabSetDiscussionResolved>(
                    "api/graphql",
                    gitlab_set_discussion_resolved::Variables {
                        discussion: DiscussionID(discussion.0),
                        resolve: *resolved,
                    },
                )?;
                ensure_errors(
                    data.discussion_toggle_resolve
                        .context("GitLab returned no result")?
                        .errors,
                )?;
                Ok(ProviderCommandOutput::Confirmation(if *resolved {
                    "Thread resolved.".to_owned()
                } else {
                    "Thread reopened.".to_owned()
                }))
            }
            ProviderCommand::ReadCiJobs => {
                let merge_request_number = self
                    .require_change_request(scope)?
                    .merge_request_number
                    .context("merge request has no IID")?;
                Ok(ProviderCommandOutput::CiJobs(
                    self.read_ci_jobs(scope, &merge_request_number)?,
                ))
            }
            ProviderCommand::ReadCiJobLog { job } => {
                self.require_ci_job(scope, job)?;
                Ok(ProviderCommandOutput::CiJobLog(self.http.read_text(
                    &format!(
                        "api/v4/projects/{}/jobs/{job}/trace",
                        encoded_project(scope.project)
                    ),
                )?))
            }
            ProviderCommand::RetryCiJob { job } => {
                self.require_ci_job(scope, job)?;
                let _: PipelineJob = self.http.post_json(
                    &format!(
                        "api/v4/projects/{}/jobs/{job}/retry",
                        encoded_project(scope.project)
                    ),
                    &serde_json::Value::Null,
                )?;
                Ok(ProviderCommandOutput::Confirmation(format!(
                    "Retry requested for job {job}."
                )))
            }
            ProviderCommand::CancelCiJob { job } => {
                self.require_ci_job(scope, job)?;
                let _: PipelineJob = self.http.post_json(
                    &format!(
                        "api/v4/projects/{}/jobs/{job}/cancel",
                        encoded_project(scope.project)
                    ),
                    &serde_json::Value::Null,
                )?;
                Ok(ProviderCommandOutput::Confirmation(format!(
                    "Cancellation requested for job {job}."
                )))
            }
            ProviderCommand::WaitForReviewOrCiChange => {
                super::wait_for_review_or_ci_change(self, scope)
            }
            ProviderCommand::CloseChangeRequest | ProviderCommand::ReopenChangeRequest => {
                let merge_request_number = self
                    .require_change_request(scope)?
                    .merge_request_number
                    .context("merge request has no IID")?;
                let state = if matches!(command, ProviderCommand::CloseChangeRequest) {
                    gitlab_update_merge_request::MergeRequestNewState::CLOSED
                } else {
                    gitlab_update_merge_request::MergeRequestNewState::OPEN
                };
                let data = self.http.execute_graphql::<GitlabUpdateMergeRequest>(
                    "api/graphql",
                    gitlab_update_merge_request::Variables {
                        project: scope.project.to_owned(),
                        iid: merge_request_number.0,
                        title: None,
                        description: None,
                        state: Some(state),
                    },
                )?;
                ensure_errors(
                    data.merge_request_update
                        .context("GitLab returned no result")?
                        .errors,
                )?;
                self.read_refreshed_change_request(scope)
            }
        }
    }
}

fn set_change_request_draft(
    http: &ProviderHttpClient,
    scope: &ProviderCommandScope<'_>,
    merge_request_number: GitlabMergeRequestNumber,
    draft: bool,
) -> Result<()> {
    let data = http.execute_graphql::<GitlabSetMergeRequestDraft>(
        "api/graphql",
        gitlab_set_merge_request_draft::Variables {
            project: scope.project.to_owned(),
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
