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

#[derive(Clone, Deserialize, Eq, PartialEq)]
struct WorkflowRun {
    id: u64,
    #[serde(default)]
    name: String,
    #[serde(default)]
    status: String,
    conclusion: Option<String>,
    html_url: Option<String>,
    #[serde(default)]
    head_sha: String,
    head_branch: Option<String>,
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

#[derive(Clone, Deserialize, Eq, PartialEq)]
struct PullRequest {
    number: u64,
    html_url: String,
    title: String,
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

    pub(crate) fn with_base_url(base_url: String, token: &str) -> Result<Self> {
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
                        run: None,
                        name: check.name,
                        state: check
                            .conclusion
                            .map(|state| format!("{state:?}").to_ascii_lowercase())
                            .unwrap_or_else(|| format!("{:?}", check.status).to_ascii_lowercase()),
                        url: check.details_url.map(|url| url.0),
                    }),
                    StatusNode::StatusContext(status) => jobs.push(CiJob {
                        handle: CiJobHandle::new(format!("S:{}", status.id)),
                        run: None,
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
                let handle = thread.id.clone();
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
                let handle = review.id.clone();
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
            let handle = comment.id.clone();
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
            bail!("the explicitly identified branch has no pull request");
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
        Ok(self.list_ci_for_commit(scope.project, scope.head)?.1)
    }

    fn read_action_job(
        &self,
        scope: &ProviderCommandScope<'_>,
        handle: &CiJobHandle,
    ) -> Result<CiJob> {
        let job_id = handle.as_str().parse::<u64>().map_err(|_| {
            anyhow::anyhow!(
                "`{handle}` is not a numeric GitHub Actions job ID; use the job ID from its GitHub Actions URL"
            )
        })?;
        let job: WorkflowJob = self.rest.read_json(&format!(
            "{}repos/{}/actions/jobs/{job_id}",
            self.rest_prefix, scope.project
        ))?;
        Ok(ci_job(job))
    }

    fn read_workflow_job(&self, project: &str, job: u64) -> Result<WorkflowJob> {
        self.rest.read_json(&format!(
            "{}repos/{project}/actions/jobs/{job}",
            self.rest_prefix
        ))
    }

    fn read_workflow_run(&self, project: &str, run: u64) -> Result<WorkflowRun> {
        self.rest.read_json(&format!(
            "{}repos/{project}/actions/runs/{run}",
            self.rest_prefix
        ))
    }

    fn read_pull_request(&self, project: &str, mr: u64) -> Result<PullRequest> {
        self.rest
            .read_json(&format!("{}repos/{project}/pulls/{mr}", self.rest_prefix))
    }

    fn project_scope_for_pull_request<'a>(
        project_scope: &'a ProviderProjectScope<'a>,
        request: &'a PullRequest,
    ) -> ProviderCommandScope<'a> {
        ProviderCommandScope {
            host: project_scope.host,
            project: project_scope.project,
            base: &request.base.reference,
            prefix: project_scope.prefix,
            branch: &request.head.reference,
            head: &request.head.sha,
        }
    }

    fn require_writable_pull_request(
        scope: &ProviderProjectScope<'_>,
        request: &PullRequest,
    ) -> Result<()> {
        if request
            .head
            .repo
            .as_ref()
            .map(|repo| repo.full_name.as_str())
            != Some(scope.project)
            || !request.head.reference.starts_with(scope.prefix)
            || request.base.reference != scope.base
        {
            bail!(
                "MR {} is outside the writable {}* -> {} scope",
                request.number,
                scope.prefix,
                scope.base
            );
        }
        Ok(())
    }

    fn require_writable_run(scope: &ProviderProjectScope<'_>, run: &WorkflowRun) -> Result<()> {
        if !run
            .head_branch
            .as_deref()
            .is_some_and(|branch| branch.starts_with(scope.prefix))
        {
            bail!(
                "CI run {} is not for a writable {}* ref",
                run.id,
                scope.prefix
            );
        }
        Ok(())
    }

    fn list_run_jobs(&self, project: &str, run: u64) -> Result<Vec<CiJob>> {
        let mut page = 1;
        let mut jobs = Vec::new();
        loop {
            let page_suffix = if page == 1 {
                String::new()
            } else {
                format!("&page={page}")
            };
            let response: WorkflowJobs = self.rest.read_json(&format!(
                "{}repos/{project}/actions/runs/{run}/jobs?filter=latest&per_page=100{page_suffix}",
                self.rest_prefix
            ))?;
            let total = response.total_count;
            let received = response.jobs.len();
            jobs.extend(response.jobs.into_iter().map(ci_job));
            if jobs.len() as u64 >= total {
                return Ok(jobs);
            }
            if received == 0 {
                return ensure_complete_rest_collection(
                    &format!("jobs in Actions workflow run {run}"),
                    total,
                    jobs.len(),
                )
                .map(|()| jobs);
            }
            page += 1;
        }
    }

    fn list_ci_for_commit(&self, project: &str, commit: &str) -> Result<(Vec<CiRun>, Vec<CiJob>)> {
        let mut page = 1;
        let mut workflow_runs = Vec::new();
        loop {
            let page_suffix = if page == 1 {
                String::new()
            } else {
                format!("&page={page}")
            };
            let response: WorkflowRuns = self.rest.read_json(&format!(
                "{}repos/{project}/actions/runs?head_sha={commit}&per_page=100{page_suffix}",
                self.rest_prefix
            ))?;
            let total = response.total_count;
            let received = response.workflow_runs.len();
            workflow_runs.extend(response.workflow_runs);
            if workflow_runs.len() as u64 >= total {
                break;
            }
            if received == 0 {
                ensure_complete_rest_collection(
                    "Actions workflow runs",
                    total,
                    workflow_runs.len(),
                )?;
            }
            page += 1;
        }
        let mut runs = Vec::new();
        let mut jobs = Vec::new();
        for run in workflow_runs {
            jobs.extend(self.list_run_jobs(project, run.id)?);
            runs.push(ci_run(run));
        }
        Ok((runs, jobs))
    }

    fn require_ci_job(
        &self,
        scope: &ProviderCommandScope<'_>,
        handle: &CiJobHandle,
    ) -> Result<CiJob> {
        self.read_action_jobs(scope)?
            .into_iter()
            .find(|job| job.handle.as_str() == handle.as_str())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "CI check `{handle}` is not a controllable GitHub Actions job for the current commit; run `ag-git ci` and use a current numeric Actions job handle"
                )
            })
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
                    "review thread `{handle}` was not found; run `ag-git list threads mr ID` and use its provider ID"
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
            ProviderCommand::ReadChangeRequestAfterPush => {
                Ok(ProviderCommandOutput::CurrentStatus(
                    self.read_change_request_snapshot(scope, false)?.request,
                ))
            }
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
                let current = self.read_action_job(scope, job)?;
                let path = format!(
                    "{}repos/{}/actions/jobs/{}/logs",
                    self.rest_prefix, scope.project, current.handle
                );
                match self.rest.read_optional_text(&path)? {
                    Some(log) => Ok(ProviderCommandOutput::CiJobLog(log)),
                    None if github_job_log_pending(&current.state) => {
                        Ok(ProviderCommandOutput::CiJobLog(format!(
                            "Job: {} ({})\nState: {}\nLog: GitHub has not published live log bytes for this running job.\n",
                            current.handle, current.name, current.state
                        )))
                    }
                    None => bail!(
                        "GitHub Actions job `{}` ({}) is {}, but its log is not available\nNext step: retry `ag-git log {job}`; if it remains unavailable, open {}",
                        current.handle,
                        current.name,
                        current.state,
                        current.url.as_deref().unwrap_or("the job in GitHub Actions")
                    ),
                }
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

    fn execute_cli_command(
        &self,
        scope: &ProviderProjectScope<'_>,
        command: &CliCommand,
    ) -> Result<ProviderCommandOutput> {
        match command {
            CliCommand::ShowMr { mr } => Ok(ProviderCommandOutput::ChangeRequest(
                pull_request_status(self.read_pull_request(scope.project, *mr)?),
            )),
            CliCommand::ShowRun { run } => Ok(ProviderCommandOutput::CiRun(ci_run(
                self.read_workflow_run(scope.project, *run)?,
            ))),
            CliCommand::ShowJob { job } => Ok(ProviderCommandOutput::CiJob(ci_job(
                self.read_workflow_job(scope.project, *job)?,
            ))),
            CliCommand::ListThreads { mr } => {
                let request = self.read_pull_request(scope.project, *mr)?;
                let current = Self::project_scope_for_pull_request(scope, &request);
                self.execute_command(&current, &ProviderCommand::ReadReviewThreads)
            }
            CliCommand::ListCi { commit } => {
                let (runs, jobs) = self.list_ci_for_commit(scope.project, commit)?;
                Ok(ProviderCommandOutput::CiRunsAndJobs { runs, jobs })
            }
            CliCommand::ListJobs { run } => Ok(ProviderCommandOutput::CiJobs(
                self.list_run_jobs(scope.project, *run)?,
            )),
            CliCommand::LogJob { job } => {
                let project_scope = ProviderCommandScope {
                    host: scope.host,
                    project: scope.project,
                    base: scope.base,
                    prefix: scope.prefix,
                    branch: "",
                    head: "",
                };
                self.execute_command(
                    &project_scope,
                    &ProviderCommand::ReadCiJobLog {
                        job: CiJobHandle::new(job.to_string()),
                    },
                )
            }
            CliCommand::WaitMr { mr } => {
                let initial = self.read_pull_request(scope.project, *mr)?;
                if matches!(initial.state.as_str(), "closed" | "merged") {
                    return Ok(ProviderCommandOutput::ChangeRequest(pull_request_status(
                        initial,
                    )));
                }
                loop {
                    std::thread::sleep(std::time::Duration::from_secs(10));
                    let current = self.read_pull_request(scope.project, *mr)?;
                    if current != initial {
                        return Ok(ProviderCommandOutput::ChangeRequest(pull_request_status(
                            current,
                        )));
                    }
                }
            }
            CliCommand::WaitRun { run } => loop {
                let current = self.read_workflow_run(scope.project, *run)?;
                let output = ci_run(current);
                if ci_terminal(&output.state) {
                    return Ok(ProviderCommandOutput::CiRun(output));
                }
                std::thread::sleep(std::time::Duration::from_secs(10));
            },
            CliCommand::WaitJob { job } => loop {
                let current = self.read_workflow_job(scope.project, *job)?;
                let output = ci_job(current);
                if ci_terminal(&output.state) {
                    return Ok(ProviderCommandOutput::CiJob(output));
                }
                std::thread::sleep(std::time::Duration::from_secs(10));
            },
            CliCommand::OpenMr { head, base, draft } => {
                if !head.starts_with(scope.prefix) || base != scope.base {
                    bail!(
                        "open mr must use a {}* head and the granted {} base",
                        scope.prefix,
                        scope.base
                    );
                }
                let encoded: String =
                    url::form_urlencoded::byte_serialize(head.as_bytes()).collect();
                let commit: Commit = self.rest.read_json(&format!(
                    "{}repos/{}/commits/{encoded}",
                    self.rest_prefix, scope.project
                ))?;
                let current = ProviderCommandScope {
                    host: scope.host,
                    project: scope.project,
                    base,
                    prefix: scope.prefix,
                    branch: head,
                    head: &commit.sha,
                };
                self.execute_command(
                    &current,
                    &ProviderCommand::OpenChangeRequest { draft: *draft },
                )
            }
            CliCommand::SetMr { mr, state } => {
                let request = self.read_pull_request(scope.project, *mr)?;
                Self::require_writable_pull_request(scope, &request)?;
                let current = Self::project_scope_for_pull_request(scope, &request);
                let provider_command = match state {
                    ChangeRequestState::Ready => ProviderCommand::MarkChangeRequestReady,
                    ChangeRequestState::Draft => ProviderCommand::MarkChangeRequestDraft,
                    ChangeRequestState::Open => ProviderCommand::ReopenChangeRequest,
                    ChangeRequestState::Closed => ProviderCommand::CloseChangeRequest,
                };
                self.execute_command(&current, &provider_command)
            }
            CliCommand::EditMr { mr, title, body } => {
                let request = self.read_pull_request(scope.project, *mr)?;
                Self::require_writable_pull_request(scope, &request)?;
                let current = Self::project_scope_for_pull_request(scope, &request);
                self.execute_command(
                    &current,
                    &ProviderCommand::EditChangeRequest {
                        title: title.clone(),
                        body: body.clone(),
                    },
                )
            }
            CliCommand::CommentMr { mr, body } => {
                let request = self.read_pull_request(scope.project, *mr)?;
                Self::require_writable_pull_request(scope, &request)?;
                let current = Self::project_scope_for_pull_request(scope, &request);
                self.execute_command(
                    &current,
                    &ProviderCommand::AddChangeRequestComment { body: body.clone() },
                )
            }
            CliCommand::ReplyThread { mr, thread, body } => {
                let request = self.read_pull_request(scope.project, *mr)?;
                Self::require_writable_pull_request(scope, &request)?;
                let current = Self::project_scope_for_pull_request(scope, &request);
                self.execute_command(
                    &current,
                    &ProviderCommand::ReplyToReviewThread {
                        thread: thread.clone(),
                        body: body.clone(),
                    },
                )
            }
            CliCommand::SetThread {
                mr,
                thread,
                resolved,
            } => {
                let request = self.read_pull_request(scope.project, *mr)?;
                Self::require_writable_pull_request(scope, &request)?;
                let current = Self::project_scope_for_pull_request(scope, &request);
                self.execute_command(
                    &current,
                    &ProviderCommand::SetReviewThreadResolved {
                        thread: thread.clone(),
                        resolved: *resolved,
                    },
                )
            }
            CliCommand::RetryJob { job } | CliCommand::CancelJob { job } => {
                let current = self.read_workflow_job(scope.project, *job)?;
                let run = self.read_workflow_run(scope.project, current.run_id)?;
                Self::require_writable_run(scope, &run)?;
                if matches!(command, CliCommand::CancelJob { .. }) {
                    bail!(
                        "GitHub cannot cancel one job; use `ag-git cancel run {}`",
                        run.id
                    );
                }
                self.rest.post_without_body(&format!(
                    "{}repos/{}/actions/jobs/{job}/rerun",
                    self.rest_prefix, scope.project
                ))?;
                Ok(ProviderCommandOutput::Confirmation(format!(
                    "Retry requested for job {job} and its dependent jobs."
                )))
            }
            CliCommand::CancelRun { run } => {
                let current = self.read_workflow_run(scope.project, *run)?;
                Self::require_writable_run(scope, &current)?;
                self.rest.post_without_body(&format!(
                    "{}repos/{}/actions/runs/{run}/cancel",
                    self.rest_prefix, scope.project
                ))?;
                Ok(ProviderCommandOutput::Confirmation(format!(
                    "Cancellation requested for run {run}."
                )))
            }
        }
    }
}

fn pull_request_status(request: PullRequest) -> ChangeRequestStatus {
    ChangeRequestStatus {
        handle: request.number.to_string(),
        url: request.html_url,
        title: request.title,
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
    use crate::api::test_server::{serve, serve_with_statuses, ExpectedRequest};

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
        assert_eq!(request.threads[0].handle.as_str(), "thread-1");
        assert_eq!(request.threads[1].handle.as_str(), "review-1");
        assert_eq!(request.threads[1].comments[0].author, "lead");
        assert_eq!(request.threads[2].handle.as_str(), "comment-1");
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
            GithubApi::review_target(&snapshot, &ReviewThreadHandle::new("thread-1")).unwrap();

        assert_eq!(
            target,
            GithubReviewTarget::Thread(GithubReviewThreadId("thread-1".to_owned()))
        );
        server.join().unwrap().unwrap();
    }

    #[test]
    fn paginates_actions_results() {
        let (base_url, server) = serve(vec![
            ExpectedRequest {
                method: "GET",
                path: "/repos/acme/widget/actions/runs?head_sha=abc123&per_page=100",
                required_header: Some(("authorization", "Bearer fixture-token")),
                body_contains: None,
                response_content_type: "application/json",
                response_body: r#"{"total_count":2,"workflow_runs":[{"id":91}]}"#,
            },
            ExpectedRequest {
                method: "GET",
                path: "/repos/acme/widget/actions/runs?head_sha=abc123&per_page=100&page=2",
                required_header: Some(("authorization", "Bearer fixture-token")),
                body_contains: None,
                response_content_type: "application/json",
                response_body: r#"{"total_count":2,"workflow_runs":[{"id":92}]}"#,
            },
            ExpectedRequest {
                method: "GET",
                path: "/repos/acme/widget/actions/runs/91/jobs?filter=latest&per_page=100",
                required_header: Some(("authorization", "Bearer fixture-token")),
                body_contains: None,
                response_content_type: "application/json",
                response_body: r#"{"total_count":0,"jobs":[]}"#,
            },
            ExpectedRequest {
                method: "GET",
                path: "/repos/acme/widget/actions/runs/92/jobs?filter=latest&per_page=100",
                required_header: Some(("authorization", "Bearer fixture-token")),
                body_contains: None,
                response_content_type: "application/json",
                response_body: r#"{"total_count":0,"jobs":[]}"#,
            },
        ]);
        let provider = GithubApi::with_base_url(base_url, "fixture-token").unwrap();

        assert!(provider.read_action_jobs(&scope()).unwrap().is_empty());
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
    fn running_job_log_can_be_read_outside_the_current_commit() {
        let (base_url, server) = serve_with_statuses(vec![
            (
                ExpectedRequest {
                    method: "GET",
                    path: "/repos/acme/widget/actions/jobs/94318091035",
                    required_header: Some(("authorization", "Bearer fixture-token")),
                    body_contains: None,
                    response_content_type: "application/json",
                    response_body: r#"{"id":94318091035,"name":"Linux","status":"in_progress","conclusion":null,"html_url":"https://github.test/jobs/94318091035","run_id":91}"#,
                },
                200,
            ),
            (
                ExpectedRequest {
                    method: "GET",
                    path: "/repos/acme/widget/actions/jobs/94318091035/logs",
                    required_header: Some(("authorization", "Bearer fixture-token")),
                    body_contains: None,
                    response_content_type: "application/xml",
                    response_body: "<Error><Code>BlobNotFound</Code></Error>",
                },
                404,
            ),
        ]);
        let provider = GithubApi::with_base_url(base_url, "fixture-token").unwrap();
        let output = provider
            .execute_command(
                &scope(),
                &ProviderCommand::ReadCiJobLog {
                    job: CiJobHandle::new("94318091035"),
                },
            )
            .unwrap();

        let ProviderCommandOutput::CiJobLog(output) = output else {
            panic!("expected a CI job log")
        };
        insta::assert_snapshot!(output, @r###"
        Job: 94318091035 (Linux)
        State: in_progress
        Log: GitHub has not published live log bytes for this running job.
        "###);
        server.join().unwrap().unwrap();
    }

    #[test]
    fn completed_job_log_is_downloaded() {
        let (base_url, server) = serve(vec![
            ExpectedRequest {
                method: "GET",
                path: "/repos/acme/widget/actions/jobs/44",
                required_header: Some(("authorization", "Bearer fixture-token")),
                body_contains: None,
                response_content_type: "application/json",
                response_body: r#"{"id":44,"name":"Linux","status":"completed","conclusion":"success","html_url":"https://github.test/jobs/44","run_id":91}"#,
            },
            ExpectedRequest {
                method: "GET",
                path: "/repos/acme/widget/actions/jobs/44/logs",
                required_header: Some(("authorization", "Bearer fixture-token")),
                body_contains: None,
                response_content_type: "text/plain",
                response_body: "build complete\n",
            },
        ]);
        let provider = GithubApi::with_base_url(base_url, "fixture-token").unwrap();

        let output = provider
            .execute_command(
                &scope(),
                &ProviderCommand::ReadCiJobLog {
                    job: CiJobHandle::new("44"),
                },
            )
            .unwrap();

        assert_eq!(
            output,
            ProviderCommandOutput::CiJobLog("build complete\n".to_owned())
        );
        server.join().unwrap().unwrap();
    }

    #[test]
    fn explicit_resource_commands_do_not_need_checkout_context() {
        let (base_url, server) = serve(vec![
            ExpectedRequest {
                method: "GET",
                path: "/repos/acme/widget/pulls/7",
                required_header: Some(("authorization", "Bearer fixture-token")),
                body_contains: None,
                response_content_type: "application/json",
                response_body: r#"{"number":7,"html_url":"https://github.test/acme/widget/pull/7","title":"Fix login","state":"open","draft":false,"head":{"ref":"wt/fix-login","sha":"abc123","repo":{"full_name":"acme/widget"}},"base":{"ref":"main","sha":"def456","repo":{"full_name":"acme/widget"}}}"#,
            },
            ExpectedRequest {
                method: "GET",
                path: "/repos/acme/widget/actions/jobs/44",
                required_header: Some(("authorization", "Bearer fixture-token")),
                body_contains: None,
                response_content_type: "application/json",
                response_body: r#"{"id":44,"name":"Linux","status":"completed","conclusion":"success","html_url":"https://github.test/jobs/44","run_id":91}"#,
            },
        ]);
        let provider = GithubApi::with_base_url(base_url, "fixture-token").unwrap();
        let scope = project_scope();

        let mr = provider
            .execute_cli_command(&scope, &CliCommand::ShowMr { mr: 7 })
            .unwrap();
        let job = provider
            .execute_cli_command(&scope, &CliCommand::WaitJob { job: 44 })
            .unwrap();

        let ProviderCommandOutput::ChangeRequest(mr) = mr else {
            panic!("expected MR")
        };
        let ProviderCommandOutput::CiJob(job) = job else {
            panic!("expected job")
        };
        assert_eq!(mr.handle, "7");
        assert_eq!(job.state, "success");
        server.join().unwrap().unwrap();
    }

    #[test]
    fn write_scope_comes_from_provider_resource_metadata() {
        let mut request = PullRequest {
            number: 7,
            html_url: String::new(),
            title: String::new(),
            state: "open".to_owned(),
            draft: false,
            head: PullRequestRef {
                reference: "main".to_owned(),
                sha: "abc123".to_owned(),
                repo: Some(PullRequestRepository {
                    full_name: "acme/widget".to_owned(),
                }),
            },
            base: PullRequestRef {
                reference: "main".to_owned(),
                sha: "def456".to_owned(),
                repo: Some(PullRequestRepository {
                    full_name: "acme/widget".to_owned(),
                }),
            },
        };
        assert!(GithubApi::require_writable_pull_request(&project_scope(), &request).is_err());
        request.head.reference = "wt/fix".to_owned();
        assert!(GithubApi::require_writable_pull_request(&project_scope(), &request).is_ok());
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

    fn project_scope() -> ProviderProjectScope<'static> {
        ProviderProjectScope {
            host: "github.test",
            project: "acme/widget",
            base: "main",
            prefix: "wt/",
        }
    }
}
