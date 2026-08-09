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

struct GithubChangeRequestSnapshot {
    repository_id: GithubRepositoryId,
    pull_request_id: Option<GithubPullRequestId>,
    request: Option<ChangeRequestStatus>,
    thread_ids: Vec<GithubReviewThreadId>,
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
    workflow_runs: Vec<WorkflowRun>,
}

#[derive(Deserialize)]
struct WorkflowRun {
    id: u64,
}

#[derive(Deserialize)]
struct WorkflowJobs {
    jobs: Vec<WorkflowJob>,
}

#[derive(Deserialize)]
struct WorkflowJob {
    id: u64,
    name: String,
    status: String,
    conclusion: Option<String>,
    html_url: Option<String>,
    run_id: u64,
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
        let mut nodes = repository
            .pull_requests
            .nodes
            .unwrap_or_default()
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        let node = nodes
            .iter()
            .position(|request| request.head_ref_oid.0 == scope.head)
            .map(|index| nodes.remove(index));
        if node.is_none() {
            if let Some(request) = nodes.first() {
                bail!(
                    "the pull request is at commit {}, but this checkout is at {}; push the branch first",
                    request.head_ref_oid.0,
                    scope.head
                );
            }
        }
        let Some(node) = node else {
            return Ok(GithubChangeRequestSnapshot {
                repository_id: GithubRepositoryId(repository.id),
                pull_request_id: None,
                request: None,
                thread_ids: Vec::new(),
            });
        };
        let mut thread_ids = Vec::new();
        let threads = node
            .review_threads
            .nodes
            .unwrap_or_default()
            .into_iter()
            .flatten()
            .enumerate()
            .map(|(index, thread)| {
                thread_ids.push(GithubReviewThreadId(thread.id.clone()));
                ReviewThread {
                    handle: ReviewThreadHandle::new(format!("T{}", index + 1)),
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
                }
            })
            .collect();
        let jobs = if include_jobs {
            self.read_ci_jobs(scope)?
        } else {
            Vec::new()
        };
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
            jobs,
        };
        Ok(GithubChangeRequestSnapshot {
            repository_id: GithubRepositoryId(repository.id),
            pull_request_id: Some(GithubPullRequestId(node.id)),
            request: Some(request),
            thread_ids,
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

    fn read_ci_jobs(&self, scope: &ProviderCommandScope<'_>) -> Result<Vec<CiJob>> {
        let runs: WorkflowRuns = self.rest.read_json(&format!(
            "{}repos/{}/actions/runs?head_sha={}&per_page=100",
            self.rest_prefix, scope.project, scope.head
        ))?;
        let mut jobs = Vec::new();
        for run in runs.workflow_runs {
            let response: WorkflowJobs = self.rest.read_json(&format!(
                "{}repos/{}/actions/runs/{}/jobs?filter=latest&per_page=100",
                self.rest_prefix, scope.project, run.id
            ))?;
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
            .read_ci_jobs(scope)?
            .iter()
            .any(|job| job.handle.as_str() == handle.as_str())
        {
            Ok(())
        } else {
            bail!("CI job `{handle}` does not belong to the current commit")
        }
    }

    fn review_thread_id(
        snapshot: &GithubChangeRequestSnapshot,
        handle: &ReviewThreadHandle,
    ) -> Result<GithubReviewThreadId> {
        let index = handle
            .as_str()
            .strip_prefix('T')
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|index| *index > 0)
            .ok_or_else(|| anyhow::anyhow!("invalid review thread handle `{handle}`"))?;
        snapshot
            .thread_ids
            .get(index - 1)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("review thread `{handle}` was not found"))
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
            bail!("GitHub API credential cannot create pull requests in {project}")
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
                            body: body.clone(),
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
                let thread = Self::review_thread_id(&snapshot, thread)?;
                self.graphql.execute_graphql::<GithubReplyToReviewThread>(
                    self.graphql_path,
                    github_reply_to_review_thread::Variables {
                        thread: thread.0,
                        body: body.clone(),
                    },
                )?;
                Ok(ProviderCommandOutput::Confirmation(
                    "Reply added.".to_owned(),
                ))
            }
            ProviderCommand::SetReviewThreadResolved { thread, resolved } => {
                let snapshot = self.require_change_request(scope)?;
                let thread = Self::review_thread_id(&snapshot, thread)?;
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
            ProviderCommand::ReadCiJobs => {
                Ok(ProviderCommandOutput::CiJobs(self.read_ci_jobs(scope)?))
            }
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
                    "Retry requested for job {job}."
                )))
            }
            ProviderCommand::CancelCiJob { job } => {
                self.require_ci_job(scope, job)?;
                let detail: WorkflowJob = self.rest.read_json(&format!(
                    "{}repos/{}/actions/jobs/{job}",
                    self.rest_prefix, scope.project
                ))?;
                self.rest.post_without_body(&format!(
                    "{}repos/{}/actions/runs/{}/cancel",
                    self.rest_prefix, scope.project, detail.run_id
                ))?;
                Ok(ProviderCommandOutput::Confirmation(format!(
                    "Cancellation requested for job {job}."
                )))
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
                    "nodes": [{
                        "id": "pull-request-7",
                        "number": 7,
                        "url": "https://github.test/acme/widget/pull/7",
                        "title": "Fix login",
                        "state": "OPEN",
                        "isDraft": false,
                        "headRefOid": "abc123",
                        "baseRefName": "main",
                        "reviewDecision": "CHANGES_REQUESTED",
                        "reviewThreads": {
                            "nodes": [{
                                "id": "thread-1",
                                "isResolved": false,
                                "path": "src/login.rs",
                                "line": 12,
                                "comments": {
                                    "nodes": [{
                                        "author": { "__typename": "User", "login": "reviewer" },
                                        "body": "Handle the error here.",
                                        "url": "https://github.test/acme/widget/pull/7#discussion"
                                    }]
                                }
                            }]
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
                "pullRequests": { "nodes": [] }
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
                response_body: r#"{"workflow_runs":[{"id":91}]}"#,
            },
            ExpectedRequest {
                method: "GET",
                path: "/repos/acme/widget/actions/runs/91/jobs?filter=latest&per_page=100",
                required_header: Some(("authorization", "Bearer fixture-token")),
                body_contains: None,
                response_content_type: "application/json",
                response_body: r#"{"jobs":[{"id":44,"name":"test","status":"completed","conclusion":"success","html_url":"https://github.test/jobs/44","run_id":91}]}"#,
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
        assert_eq!(request.jobs[0].handle, CiJobHandle::new("44"));
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
}
