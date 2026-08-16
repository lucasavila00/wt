mod provider;

use super::*;

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

    pub(super) fn read_change_request_snapshot(
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

    pub(super) fn read_action_jobs(&self, scope: &ProviderCommandScope<'_>) -> Result<Vec<CiJob>> {
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

    fn read_check_run_annotations(
        &self,
        project: &str,
        check_run: &CiJobHandle,
    ) -> Result<Vec<CheckRunAnnotation>> {
        let mut page = 1;
        let mut annotations = Vec::new();
        loop {
            let current: Vec<CheckRunAnnotation> = self.rest.read_json(&format!(
                "{}repos/{project}/check-runs/{check_run}/annotations?per_page=100&page={page}",
                self.rest_prefix
            ))?;
            let received = current.len();
            annotations.extend(current);
            if received < 100 {
                return Ok(annotations);
            }
            page += 1;
        }
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

    fn read_open_pull_request_for_branch(
        &self,
        project: &str,
        base: &str,
        branch: &str,
    ) -> Result<PullRequest> {
        let (owner, _) = split_project(project)?;
        let head = url::form_urlencoded::byte_serialize(format!("{owner}:{branch}").as_bytes())
            .collect::<String>();
        let base_query = url::form_urlencoded::byte_serialize(base.as_bytes()).collect::<String>();
        let mut requests: Vec<PullRequest> = self.rest.read_json(&format!(
            "{}repos/{project}/pulls?state=open&head={head}&base={base_query}&per_page=100",
            self.rest_prefix
        ))?;
        requests.retain(|request| {
            request.state == "open"
                && request.head.reference == branch
                && request
                    .head
                    .repo
                    .as_ref()
                    .is_some_and(|repository| repository.full_name == project)
                && request.base.reference == base
        });
        match requests.len() {
            0 => bail!("no open pull request from branch `{branch}` to `{base}`"),
            1 => Ok(requests.pop().expect("one pull request remains")),
            count => bail!(
                "GitHub returned {count} open pull requests from branch `{branch}` to `{base}`; refusing to choose one"
            ),
        }
    }

    fn read_review_threads(&self, project: &str, mr: u64) -> Result<Vec<ReviewThread>> {
        let (owner, name) = split_project(project)?;
        let data = self
            .graphql
            .execute_graphql::<GithubReadPullRequestByNumber>(
                self.graphql_path,
                github_read_pull_request_by_number::Variables {
                    owner: owner.to_owned(),
                    name: name.to_owned(),
                    number: i64::try_from(mr).context("GitHub pull request number is too large")?,
                },
            )?;
        let request = data
            .repository
            .with_context(|| format!("GitHub repository {project} was not found"))?
            .pull_request
            .with_context(|| format!("GitHub pull request {mr} was not found"))?;
        ensure_complete_connection(
            "review threads",
            request.review_threads.page_info.has_next_page,
            request.review_threads.total_count,
        )?;
        request
            .review_threads
            .nodes
            .unwrap_or_default()
            .into_iter()
            .flatten()
            .map(|thread| {
                ensure_complete_connection(
                    &format!("comments in review thread {}", thread.id),
                    thread.comments.page_info.has_next_page,
                    thread.comments.total_count,
                )?;
                Ok(ReviewThread {
                    handle: ReviewThreadHandle::new(thread.id),
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
            .collect()
    }

    pub(super) fn require_writable_pull_request(
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
                    "CI check `{handle}` is not a controllable GitHub Actions job for the current commit; run a `list_ci` JSON action and use a current numeric Actions job handle"
                )
            })
    }

    pub(super) fn review_target(
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
                    "review thread `{handle}` was not found; run a `list_threads` JSON action for the MR and use its provider ID"
                )
            })
    }
}
