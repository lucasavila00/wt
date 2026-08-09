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

pub(crate) struct GithubApi {
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

#[derive(Deserialize)]
struct WorkflowRun {
    id: u64,
}

#[derive(Deserialize)]
struct WorkflowJobs {
    total_count: u64,
    jobs: Vec<WorkflowJob>,
}

#[derive(Deserialize)]
struct WorkflowJob {
    id: u64,
    name: String,
    status: String,
    conclusion: Option<String>,
    html_url: Option<String>,
}

impl GithubApi {
    pub(crate) fn new(host: &str, token: &str) -> Result<Self> {
        let (base, graphql_path, rest_prefix) = if host == "github.com" {
            ("https://api.github.com".to_owned(), "graphql", "")
        } else {
            (format!("https://{host}"), "api/graphql", "api/v3/")
        };
        Ok(Self {
            graphql: ProviderHttpClient::new(base.clone(), token, ProviderAuthentication::Github)?,
            rest: ProviderHttpClient::new(base, token, ProviderAuthentication::Github)?,
            graphql_path,
            rest_prefix,
        })
    }

    #[cfg(test)]
    fn with_base_url(base_url: String, token: &str) -> Result<Self> {
        Ok(Self {
            graphql: ProviderHttpClient::new(
                base_url.clone(),
                token,
                ProviderAuthentication::Github,
            )?,
            rest: ProviderHttpClient::new(base_url, token, ProviderAuthentication::Github)?,
            graphql_path: "graphql",
            rest_prefix: "",
        })
    }

    fn read_change_request_snapshot(
        &self,
        scope: &ProviderCommandScope<'_>,
        include_jobs: bool,
    ) -> Result<GithubChangeRequestSnapshot> {
        let (owner, name) = split_project(scope.project)?;
        let data = self.graphql.execute_graphql::<GithubReadPullRequest>(
            self.graphql_path,
            github_read_pull_request::Variables {
                owner: owner.to_owned(),
                name: name.to_owned(),
                branch: scope.branch.to_owned(),
                base: scope.base.to_owned(),
            },
        )?;
        let repository = data
            .repository
            .with_context(|| format!("GitHub repository {} was not found", scope.project))?;
        ensure_complete_connection(
            "pull requests",
            repository.pull_requests.page_info.has_next_page,
            repository.pull_requests.total_count,
        )?;
        let mut jobs: Vec<CiJob> = Vec::new();
        let repository_id = GithubRepositoryId(repository.id);
        let mut nodes = repository
            .pull_requests
            .nodes
            .unwrap_or_default()
            .into_iter()
            .flatten()
            .filter(|request| {
                !request.is_cross_repository
                    && request
                        .head_repository
                        .as_ref()
                        .is_some_and(|head| head.id == repository_id.0)
            })
            .collect::<Vec<_>>();
        let node = nodes
            .iter()
            .position(|request| request.head_ref_oid.0 == scope.head)
            .map(|index| nodes.remove(index));
        let Some(mut node) = node else {
            if include_jobs {
                jobs.extend(self.read_action_jobs(scope)?);
            }
            return Ok(GithubChangeRequestSnapshot {
                repository_id,
                pull_request_id: None,
                request: None,
                review_targets: Vec::new(),
                jobs,
            });
        };
        if let Some(rollup) = include_jobs
            .then(|| node.status_check_rollup.take())
            .flatten()
        {
            ensure_complete_connection(
                "status checks",
                rollup.contexts.page_info.has_next_page,
                rollup.contexts.total_count,
            )?;
            for context in rollup
                .contexts
                .nodes
                .unwrap_or_default()
                .into_iter()
                .flatten()
            {
                use github_read_pull_request::GithubReadPullRequestRepositoryPullRequestsNodesStatusCheckRollupContextsNodes as StatusNode;
                match context {
                    StatusNode::CheckRun(check) => jobs.push(CiJob {
                        handle: CiJobHandle::new(
                            check
                                .database_id
                                .map(|id| id.to_string())
                                .unwrap_or_else(|| format!("C:{}", check.id)),
                        ),
                        name: check.name,
                        state: check
                            .conclusion
                            .map(|state| format!("{state:?}").to_ascii_lowercase())
                            .unwrap_or_else(|| format!("{:?}", check.status).to_ascii_lowercase()),
                        url: check.details_url.map(|url| url.0),
                    }),
                    StatusNode::StatusContext(status) => jobs.push(CiJob {
                        handle: CiJobHandle::new(format!("S:{}", status.id)),
                        name: status.context,
                        state: format!("{:?}", status.state).to_ascii_lowercase(),
                        url: status.target_url.map(|url| url.0),
                    }),
                }
            }
        }
        if include_jobs {
            for job in self.read_action_jobs(scope)? {
                if let Some(existing) = jobs
                    .iter_mut()
                    .find(|existing| existing.handle == job.handle)
                {
                    *existing = job;
                } else {
                    jobs.push(job);
                }
            }
        }
        ensure_complete_connection(
            "review threads",
            node.review_threads.page_info.has_next_page,
            node.review_threads.total_count,
        )?;
        ensure_complete_connection(
            "pull request comments",
            node.comments.page_info.has_next_page,
            node.comments.total_count,
        )?;
        let pull_request_id = GithubPullRequestId(node.id.clone());
        let mut review_targets = Vec::new();
        let mut threads: Vec<ReviewThread> = node
            .review_threads
            .nodes
            .unwrap_or_default()
            .into_iter()
            .flatten()
            .enumerate()
            .map(|(index, thread)| {
                ensure_complete_connection(
                    &format!("comments in review thread T{}", index + 1),
                    thread.comments.page_info.has_next_page,
                    thread.comments.total_count,
                )?;
                let handle = format!("T:{}", thread.id);
                review_targets.push((
                    handle.clone(),
                    GithubReviewTarget::Thread(GithubReviewThreadId(thread.id.clone())),
                ));
                Ok(ReviewThread {
                    handle: ReviewThreadHandle::new(handle),
                    resolvable: true,
                    resolved: thread.is_resolved,
                    path: Some(thread.path),
                    line: thread.line.map(|line| line as u64),
                    comments: thread
                        .comments
                        .nodes
                        .unwrap_or_default()
                        .into_iter()
                        .flatten()
                        .map(|comment| ReviewComment {
                            author: comment
                                .author
                                .map(|author| author.login)
                                .unwrap_or_else(|| "unknown".to_owned()),
                            body: comment.body,
                            url: Some(comment.url.0),
                        })
                        .collect(),
                })
            })
            .collect::<Result<_>>()?;
        if let Some(reviews) = node.reviews {
            ensure_complete_connection(
                "pull request reviews",
                reviews.page_info.has_next_page,
                reviews.total_count,
            )?;
            for review in reviews.nodes.unwrap_or_default().into_iter().flatten() {
                let handle = format!("R:{}", review.id);
                review_targets.push((
                    handle.clone(),
                    GithubReviewTarget::PullRequest(pull_request_id.clone()),
                ));
                let state = format!("{:?}", review.state).to_ascii_lowercase();
                threads.push(ReviewThread {
                    handle: ReviewThreadHandle::new(handle),
                    resolvable: false,
                    resolved: matches!(state.as_str(), "approved" | "dismissed"),
                    path: Some("pull request review".to_owned()),
                    line: None,
                    comments: vec![ReviewComment {
                        author: review
                            .author
                            .map(|author| author.login)
                            .unwrap_or_else(|| "unknown".to_owned()),
                        body: if review.body.trim().is_empty() {
                            format!("Review state: {state}.")
                        } else {
                            format!("Review state: {state}.\n\n{}", review.body)
                        },
                        url: Some(review.url.0),
                    }],
                });
            }
        }
        for comment in node
            .comments
            .nodes
            .unwrap_or_default()
            .into_iter()
            .flatten()
        {
            let handle = format!("C:{}", comment.id);
            review_targets.push((
                handle.clone(),
                GithubReviewTarget::PullRequest(pull_request_id.clone()),
            ));
            threads.push(ReviewThread {
                handle: ReviewThreadHandle::new(handle),
                resolvable: false,
                resolved: false,
                path: Some("pull request comment".to_owned()),
                line: None,
                comments: vec![ReviewComment {
                    author: comment
                        .author
                        .map(|author| author.login)
                        .unwrap_or_else(|| "unknown".to_owned()),
                    body: comment.body,
                    url: Some(comment.url.0),
                }],
            });
        }
        let request = ChangeRequestStatus {
            handle: format!("#{}", node.number),
            url: node.url.0,
            title: node.title,
            state: format!("{:?}", node.state).to_ascii_lowercase(),
            draft: node.is_draft,
            head: node.head_ref_oid.0,
            base: node.base_ref_name,
            review_state: node
                .review_decision
                .map(|state| format!("{state:?}").to_ascii_lowercase()),
            threads,
            jobs: jobs.clone(),
        };
        Ok(GithubChangeRequestSnapshot {
            repository_id,
            pull_request_id: Some(pull_request_id),
            request: Some(request),
            review_targets,
            jobs,
        })
    }

    fn require_change_request(
        &self,
        scope: &ProviderCommandScope<'_>,
    ) -> Result<GithubChangeRequestSnapshot> {
        let snapshot = self.read_change_request_snapshot(scope, false)?;
        if snapshot.request.is_none() {
            bail!("this branch has no pull request; run `ag-git open-mr`");
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
                .context("pull request disappeared")?,
        ))
    }

    fn read_action_jobs(&self, scope: &ProviderCommandScope<'_>) -> Result<Vec<CiJob>> {
        let runs: WorkflowRuns = self.rest.read_json(&format!(
            "{}repos/{}/actions/runs?head_sha={}&per_page=100",
            self.rest_prefix, scope.project, scope.head
        ))?;
        ensure_complete_rest_collection(
            "Actions workflow runs",
            runs.total_count,
            runs.workflow_runs.len(),
        )?;
        let mut jobs = Vec::new();
        for run in runs.workflow_runs {
            let response: WorkflowJobs = self.rest.read_json(&format!(
                "{}repos/{}/actions/runs/{}/jobs?filter=latest&per_page=100",
                self.rest_prefix, scope.project, run.id
            ))?;
            ensure_complete_rest_collection(
                &format!("jobs in Actions workflow run {}", run.id),
                response.total_count,
                response.jobs.len(),
            )?;
            jobs.extend(response.jobs.into_iter().map(|job| CiJob {
                handle: CiJobHandle::new(job.id.to_string()),
                name: job.name,
                state: job.conclusion.unwrap_or(job.status),
                url: job.html_url,
            }));
        }
        Ok(jobs)
    }

    fn require_ci_job(&self, scope: &ProviderCommandScope<'_>, handle: &CiJobHandle) -> Result<()> {
        if self
            .read_action_jobs(scope)?
            .iter()
            .any(|job| job.handle.as_str() == handle.as_str())
        {
            Ok(())
        } else {
            bail!(
                "CI check `{handle}` is not a controllable GitHub Actions job for the current commit; run `ag-git ci` and use a current numeric Actions job handle"
            )
        }
    }

    fn review_target(
        snapshot: &GithubChangeRequestSnapshot,
        handle: &ReviewThreadHandle,
    ) -> Result<GithubReviewTarget> {
        snapshot
            .review_targets
            .iter()
            .find(|(candidate, _)| candidate == handle.as_str())
            .map(|(_, target)| target.clone())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "review thread `{handle}` was not found; run `ag-git review` and use a current thread handle"
                )
            })
    }
}

impl GitProviderApi for GithubApi {
    fn verify_repository_access(&self, project: &str, base: &str) -> Result<()> {
        let (owner, name) = split_project(project)?;
        let data = self.graphql.execute_graphql::<GithubReadPullRequest>(
            self.graphql_path,
            github_read_pull_request::Variables {
                owner: owner.to_owned(),
                name: name.to_owned(),
                branch: "__wt_access_check__".to_owned(),
                base: base.to_owned(),
            },
        )?;
        let repository = data
            .repository
            .with_context(|| format!("GitHub repository {project} was not found"))?;
        let permission = repository
            .viewer_permission
            .context("GitHub did not report repository permission")?;
        if matches!(
            permission,
            github_read_pull_request::RepositoryPermission::WRITE
                | github_read_pull_request::RepositoryPermission::MAINTAIN
                | github_read_pull_request::RepositoryPermission::ADMIN
        ) {
            Ok(())
        } else {
            bail!(
                "GitHub API credential cannot create pull requests in {project}; install a credential with write access and rerun `wt-server-setup`"
            )
        }
    }

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
                self.graphql.execute_graphql::<GithubCreatePullRequest>(
                    self.graphql_path,
                    github_create_pull_request::Variables {
                        repository: snapshot.repository_id.0,
                        base: scope.base.to_owned(),
                        branch: scope.branch.to_owned(),
                        title: title_from_branch(scope),
                        draft: *draft,
                    },
                )?;
                self.read_refreshed_change_request(scope)
            }
            ProviderCommand::MarkChangeRequestReady => {
                let id = self
                    .require_change_request(scope)?
                    .pull_request_id
                    .context("pull request has no ID")?;
                self.graphql.execute_graphql::<GithubMarkPullRequestReady>(
                    self.graphql_path,
                    github_mark_pull_request_ready::Variables { id: id.0 },
                )?;
                self.read_refreshed_change_request(scope)
            }
            ProviderCommand::MarkChangeRequestDraft => {
                let id = self
                    .require_change_request(scope)?
                    .pull_request_id
                    .context("pull request has no ID")?;
                self.graphql.execute_graphql::<GithubMarkPullRequestDraft>(
                    self.graphql_path,
                    github_mark_pull_request_draft::Variables { id: id.0 },
                )?;
                self.read_refreshed_change_request(scope)
            }
            ProviderCommand::AddChangeRequestComment { body } => {
                let id = self
                    .require_change_request(scope)?
                    .pull_request_id
                    .context("pull request has no ID")?;
                self.graphql
                    .execute_graphql::<GithubAddPullRequestComment>(
                        self.graphql_path,
                        github_add_pull_request_comment::Variables {
                            id: id.0,
                            body: super::attributed_comment(scope, body),
                        },
                    )?;
                Ok(ProviderCommandOutput::Confirmation(
                    "Comment added.".to_owned(),
                ))
            }
            ProviderCommand::EditChangeRequest { title, body } => {
                let id = self
                    .require_change_request(scope)?
                    .pull_request_id
                    .context("pull request has no ID")?;
                self.graphql.execute_graphql::<GithubUpdatePullRequest>(
                    self.graphql_path,
                    github_update_pull_request::Variables {
                        id: id.0,
                        title: title.clone(),
                        body: body.clone(),
                        state: None,
                    },
                )?;
                self.read_refreshed_change_request(scope)
            }
            ProviderCommand::ReadReviewThreads => Ok(ProviderCommandOutput::ReviewThreads(
                self.require_change_request(scope)?
                    .request
                    .context("pull request disappeared")?
                    .threads,
            )),
            ProviderCommand::ReplyToReviewThread { thread, body } => {
                let snapshot = self.require_change_request(scope)?;
                match Self::review_target(&snapshot, thread)? {
                    GithubReviewTarget::Thread(thread) => {
                        self.graphql.execute_graphql::<GithubReplyToReviewThread>(
                            self.graphql_path,
                            github_reply_to_review_thread::Variables {
                                thread: thread.0,
                                body: super::attributed_comment(scope, body),
                            },
                        )?;
                    }
                    GithubReviewTarget::PullRequest(id) => {
                        self.graphql
                            .execute_graphql::<GithubAddPullRequestComment>(
                                self.graphql_path,
                                github_add_pull_request_comment::Variables {
                                    id: id.0,
                                    body: super::attributed_comment(scope, body),
                                },
                            )?;
                    }
                }
                Ok(ProviderCommandOutput::Confirmation(
                    "Reply added.".to_owned(),
                ))
            }
            ProviderCommand::SetReviewThreadResolved { thread, resolved } => {
                let snapshot = self.require_change_request(scope)?;
                let GithubReviewTarget::Thread(thread) = Self::review_target(&snapshot, thread)?
                else {
                    bail!(
                        "feedback `{thread}` is a pull request review or comment and cannot be resolved; reply to it or address the reviewer in a new comment"
                    );
                };
                if *resolved {
                    self.graphql.execute_graphql::<GithubResolveReviewThread>(
                        self.graphql_path,
                        github_resolve_review_thread::Variables { thread: thread.0 },
                    )?;
                } else {
                    self.graphql.execute_graphql::<GithubReopenReviewThread>(
                        self.graphql_path,
                        github_reopen_review_thread::Variables { thread: thread.0 },
                    )?;
                }
                Ok(ProviderCommandOutput::Confirmation(if *resolved {
                    "Thread resolved.".to_owned()
                } else {
                    "Thread reopened.".to_owned()
                }))
            }
            ProviderCommand::ReadCiJobs => Ok(ProviderCommandOutput::CiJobs(
                self.read_change_request_snapshot(scope, true)?.jobs,
            )),
            ProviderCommand::ReadCiJobLog { job } => {
                self.require_ci_job(scope, job)?;
                Ok(ProviderCommandOutput::CiJobLog(self.rest.read_text(
                    &format!(
                        "{}repos/{}/actions/jobs/{job}/logs",
                        self.rest_prefix, scope.project
                    ),
                )?))
            }
            ProviderCommand::RetryCiJob { job } => {
                self.require_ci_job(scope, job)?;
                self.rest.post_without_body(&format!(
                    "{}repos/{}/actions/jobs/{job}/rerun",
                    self.rest_prefix, scope.project
                ))?;
                Ok(ProviderCommandOutput::Confirmation(format!(
                    "Retry requested for job {job} and its dependent jobs."
                )))
            }
            ProviderCommand::CancelCiJob { job } => {
                self.require_ci_job(scope, job)?;
                bail!(
                    "GitHub cannot cancel one Actions job: its API can only cancel the entire workflow run, including sibling jobs; ag-git refuses to widen `cancel {job}` beyond the selected job"
                )
            }
            ProviderCommand::WaitForReviewOrCiChange => {
                super::wait_for_review_or_ci_change(self, scope)
            }
            ProviderCommand::CloseChangeRequest | ProviderCommand::ReopenChangeRequest => {
                let id = self
                    .require_change_request(scope)?
                    .pull_request_id
                    .context("pull request has no ID")?;
                let state = if matches!(command, ProviderCommand::CloseChangeRequest) {
                    github_update_pull_request::PullRequestUpdateState::CLOSED
                } else {
                    github_update_pull_request::PullRequestUpdateState::OPEN
                };
                self.graphql.execute_graphql::<GithubUpdatePullRequest>(
                    self.graphql_path,
                    github_update_pull_request::Variables {
                        id: id.0,
                        title: None,
                        body: None,
                        state: Some(state),
                    },
                )?;
                self.read_refreshed_change_request(scope)
            }
        }
    }
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
            "GitHub returned only the first page of {name} ({total_count} total); ag-git refuses to continue with incomplete handles or status"
        );
    }
    Ok(())
}

fn ensure_complete_rest_collection(name: &str, total_count: u64, received: usize) -> Result<()> {
    if total_count != received as u64 {
        bail!(
            "GitHub returned only {received} of {total_count} {name}; ag-git refuses to continue with incomplete CI handles or status"
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
mod tests {
    use super::*;
    use crate::api::test_server::{serve, ExpectedRequest};

    const PULL_REQUEST_RESPONSE: &str = r#"{
        "data": {
            "viewer": { "login": "agent" },
            "repository": {
                "id": "repository-1",
                "nameWithOwner": "acme/widget",
                "viewerPermission": "WRITE",
                "pullRequests": {
                    "pageInfo": { "hasNextPage": false },
                    "totalCount": 1,
                    "nodes": [{
                        "id": "pull-request-7",
                        "number": 7,
                        "url": "https://github.test/acme/widget/pull/7",
                        "title": "Fix login",
                        "state": "OPEN",
                        "isDraft": false,
                        "headRefOid": "abc123",
                        "headRepository": { "id": "repository-1", "nameWithOwner": "acme/widget" },
                        "isCrossRepository": false,
                        "baseRefName": "main",
                        "reviewDecision": "CHANGES_REQUESTED",
                        "reviewThreads": {
                            "pageInfo": { "hasNextPage": false },
                            "totalCount": 1,
                            "nodes": [{
                                "id": "thread-1",
                                "isResolved": false,
                                "path": "src/login.rs",
                                "line": 12,
                                "comments": {
                                    "pageInfo": { "hasNextPage": false },
                                    "totalCount": 1,
                                    "nodes": [{
                                        "author": { "__typename": "User", "login": "reviewer" },
                                        "body": "Handle the error here.",
                                        "url": "https://github.test/acme/widget/pull/7#discussion"
                                    }]
                                }
                            }]
                        },
                        "reviews": {
                            "pageInfo": { "hasNextPage": false },
                            "totalCount": 1,
                            "nodes": [{
                                "id": "review-1",
                                "author": { "__typename": "User", "login": "lead" },
                                "body": "Please cover the edge case.",
                                "state": "CHANGES_REQUESTED",
                                "url": "https://github.test/acme/widget/pull/7#review"
                            }]
                        },
                        "comments": {
                            "pageInfo": { "hasNextPage": false },
                            "totalCount": 1,
                            "nodes": [{
                                "id": "comment-1",
                                "author": { "__typename": "User", "login": "maintainer" },
                                "body": "Please update the documentation too.",
                                "url": "https://github.test/acme/widget/pull/7#issuecomment-1"
                            }]
                        },
                        "statusCheckRollup": {
                            "contexts": {
                                "pageInfo": { "hasNextPage": false },
                                "totalCount": 2,
                                "nodes": [
                                    {
                                        "__typename": "CheckRun",
                                        "id": "check-44",
                                        "databaseId": 44,
                                        "name": "test",
                                        "status": "COMPLETED",
                                        "conclusion": "SUCCESS",
                                        "detailsUrl": "https://github.test/checks/44"
                                    },
                                    {
                                        "__typename": "StatusContext",
                                        "id": "status-1",
                                        "context": "external/lint",
                                        "state": "PENDING",
                                        "targetUrl": "https://ci.test/lint"
                                    }
                                ]
                            }
                        }
                    }]
                }
            }
        }
    }"#;

    const NO_PULL_REQUEST_RESPONSE: &str = r#"{
        "data": {
            "viewer": { "login": "agent" },
            "repository": {
                "id": "repository-1",
                "nameWithOwner": "acme/widget",
                "viewerPermission": "WRITE",
                "pullRequests": {
                    "pageInfo": { "hasNextPage": false },
                    "totalCount": 0,
                    "nodes": []
                }
            }
        }
    }"#;

    #[test]
    fn reads_pull_request_reviews_and_ci_from_local_fixture() {
        let (base_url, server) = serve(vec![
            ExpectedRequest {
                method: "POST",
                path: "/graphql",
                required_header: Some(("authorization", "Bearer fixture-token")),
                body_contains: Some("GithubReadPullRequest"),
                response_content_type: "application/json",
                response_body: PULL_REQUEST_RESPONSE,
            },
            ExpectedRequest {
                method: "GET",
                path: "/repos/acme/widget/actions/runs?head_sha=abc123&per_page=100",
                required_header: Some(("authorization", "Bearer fixture-token")),
                body_contains: None,
                response_content_type: "application/json",
                response_body: r#"{"total_count":1,"workflow_runs":[{"id":91}]}"#,
            },
            ExpectedRequest {
                method: "GET",
                path: "/repos/acme/widget/actions/runs/91/jobs?filter=latest&per_page=100",
                required_header: Some(("authorization", "Bearer fixture-token")),
                body_contains: None,
                response_content_type: "application/json",
                response_body: r#"{"total_count":1,"jobs":[{"id":44,"name":"test","status":"completed","conclusion":"success","html_url":"https://github.test/jobs/44","run_id":91}]}"#,
            },
        ]);
        let provider = GithubApi::with_base_url(base_url, "fixture-token").unwrap();
        let scope = scope();

        let output = provider
            .execute_command(&scope, &ProviderCommand::ReadCurrentStatus)
            .unwrap();

        let ProviderCommandOutput::CurrentStatus(Some(request)) = output else {
            panic!("expected current pull request status");
        };
        assert_eq!(request.handle, "#7");
        assert_eq!(request.threads[0].comments[0].author, "reviewer");
        assert_eq!(request.threads[0].handle.as_str(), "T:thread-1");
        assert_eq!(request.threads[1].handle.as_str(), "R:review-1");
        assert_eq!(request.threads[1].comments[0].author, "lead");
        assert_eq!(request.threads[2].handle.as_str(), "C:comment-1");
        assert_eq!(request.threads[2].comments[0].author, "maintainer");
        assert_eq!(request.jobs[0].handle, CiJobHandle::new("44"));
        assert_eq!(request.jobs[1].handle, CiJobHandle::new("S:status-1"));
        server.join().unwrap().unwrap();
    }

    #[test]
    fn refuses_cross_repository_pull_request_even_when_branch_and_commit_match() {
        let response = leak_fixture(PULL_REQUEST_RESPONSE.replace(
            "\"isCrossRepository\": false",
            "\"isCrossRepository\": true",
        ));
        let (base_url, server) = serve(vec![graphql_fixture(response)]);
        let provider = GithubApi::with_base_url(base_url, "fixture-token").unwrap();

        let snapshot = provider
            .read_change_request_snapshot(&scope(), false)
            .unwrap();

        assert!(snapshot.request.is_none());
        server.join().unwrap().unwrap();
    }

    #[test]
    fn merged_pull_request_remains_visible() {
        let response = leak_fixture(
            PULL_REQUEST_RESPONSE.replace("\"state\": \"OPEN\"", "\"state\": \"MERGED\""),
        );
        let (base_url, server) = serve(vec![graphql_fixture(response)]);
        let provider = GithubApi::with_base_url(base_url, "fixture-token").unwrap();

        let snapshot = provider
            .read_change_request_snapshot(&scope(), false)
            .unwrap();

        assert_eq!(snapshot.request.unwrap().state, "merged");
        server.join().unwrap().unwrap();
    }

    #[test]
    fn refuses_truncated_graphql_connections_instead_of_issuing_unsafe_handles() {
        let response = leak_fixture(PULL_REQUEST_RESPONSE.replacen(
            "\"hasNextPage\": false",
            "\"hasNextPage\": true",
            1,
        ));
        let (base_url, server) = serve(vec![graphql_fixture(response)]);
        let provider = GithubApi::with_base_url(base_url, "fixture-token").unwrap();

        let error = provider
            .read_change_request_snapshot(&scope(), false)
            .unwrap_err();

        assert_eq!(
            error.to_string(),
            "GitHub returned only the first page of pull requests (1 total); ag-git refuses to continue with incomplete handles or status"
        );
        server.join().unwrap().unwrap();
    }

    #[test]
    fn stable_review_handle_selects_provider_thread_after_reordering() {
        let (base_url, server) = serve(vec![graphql_fixture(PULL_REQUEST_RESPONSE)]);
        let provider = GithubApi::with_base_url(base_url, "fixture-token").unwrap();
        let mut snapshot = provider
            .read_change_request_snapshot(&scope(), false)
            .unwrap();
        snapshot.review_targets.reverse();

        let target =
            GithubApi::review_target(&snapshot, &ReviewThreadHandle::new("T:thread-1")).unwrap();

        assert_eq!(
            target,
            GithubReviewTarget::Thread(GithubReviewThreadId("thread-1".to_owned()))
        );
        server.join().unwrap().unwrap();
    }

    #[test]
    fn refuses_truncated_actions_results_instead_of_issuing_incomplete_handles() {
        let (base_url, server) = serve(vec![ExpectedRequest {
            method: "GET",
            path: "/repos/acme/widget/actions/runs?head_sha=abc123&per_page=100",
            required_header: Some(("authorization", "Bearer fixture-token")),
            body_contains: None,
            response_content_type: "application/json",
            response_body: r#"{"total_count":2,"workflow_runs":[{"id":91}]}"#,
        }]);
        let provider = GithubApi::with_base_url(base_url, "fixture-token").unwrap();

        let error = provider.read_action_jobs(&scope()).unwrap_err();

        assert_eq!(
            error.to_string(),
            "GitHub returned only 1 of 2 Actions workflow runs; ag-git refuses to continue with incomplete CI handles or status"
        );
        server.join().unwrap().unwrap();
    }

    #[test]
    fn refuses_to_turn_job_cancellation_into_whole_run_cancellation() {
        let (base_url, server) = serve(vec![
            ExpectedRequest {
                method: "GET",
                path: "/repos/acme/widget/actions/runs?head_sha=abc123&per_page=100",
                required_header: Some(("authorization", "Bearer fixture-token")),
                body_contains: None,
                response_content_type: "application/json",
                response_body: r#"{"total_count":1,"workflow_runs":[{"id":91}]}"#,
            },
            ExpectedRequest {
                method: "GET",
                path: "/repos/acme/widget/actions/runs/91/jobs?filter=latest&per_page=100",
                required_header: Some(("authorization", "Bearer fixture-token")),
                body_contains: None,
                response_content_type: "application/json",
                response_body: r#"{"total_count":1,"jobs":[{"id":44,"name":"test","status":"in_progress","conclusion":null,"html_url":"https://github.test/jobs/44","run_id":91}]}"#,
            },
        ]);
        let provider = GithubApi::with_base_url(base_url, "fixture-token").unwrap();

        let error = provider
            .execute_command(
                &scope(),
                &ProviderCommand::CancelCiJob {
                    job: CiJobHandle::new("44"),
                },
            )
            .unwrap_err();

        assert!(error
            .to_string()
            .contains("only cancel the entire workflow run"));
        server.join().unwrap().unwrap();
    }

    #[test]
    fn opens_pull_request_through_typed_graphql_mutation() {
        let (base_url, server) = serve(vec![
            ExpectedRequest {
                method: "POST",
                path: "/graphql",
                required_header: Some(("authorization", "Bearer fixture-token")),
                body_contains: Some("GithubReadPullRequest"),
                response_content_type: "application/json",
                response_body: NO_PULL_REQUEST_RESPONSE,
            },
            ExpectedRequest {
                method: "POST",
                path: "/graphql",
                required_header: Some(("authorization", "Bearer fixture-token")),
                body_contains: Some("GithubCreatePullRequest"),
                response_content_type: "application/json",
                response_body: r#"{"data":{"createPullRequest":{"pullRequest":{"id":"pull-request-7","number":7,"url":"https://github.test/acme/widget/pull/7","title":"Fix login","state":"OPEN","isDraft":false,"headRefOid":"abc123","baseRefName":"main","reviewDecision":null}}}}"#,
            },
            ExpectedRequest {
                method: "POST",
                path: "/graphql",
                required_header: Some(("authorization", "Bearer fixture-token")),
                body_contains: Some("GithubReadPullRequest"),
                response_content_type: "application/json",
                response_body: PULL_REQUEST_RESPONSE,
            },
        ]);
        let provider = GithubApi::with_base_url(base_url, "fixture-token").unwrap();

        let output = provider
            .execute_command(
                &scope(),
                &ProviderCommand::OpenChangeRequest { draft: false },
            )
            .unwrap();

        let ProviderCommandOutput::ChangeRequest(request) = output else {
            panic!("expected opened pull request");
        };
        assert_eq!(request.handle, "#7");
        server.join().unwrap().unwrap();
    }

    fn scope() -> ProviderCommandScope<'static> {
        ProviderCommandScope {
            host: "github.test",
            project: "acme/widget",
            base: "main",
            prefix: "df1/",
            branch: "df1/fix-login",
            head: "abc123",
        }
    }

    fn graphql_fixture(response_body: &'static str) -> ExpectedRequest {
        ExpectedRequest {
            method: "POST",
            path: "/graphql",
            required_header: Some(("authorization", "Bearer fixture-token")),
            body_contains: Some("GithubReadPullRequest"),
            response_content_type: "application/json",
            response_body,
        }
    }

    fn leak_fixture(value: String) -> &'static str {
        Box::leak(value.into_boxed_str())
    }
}
