use super::*;
use crate::api::{cli_wait_deadline, wait_for_next_cli_poll};

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
                "GitHub API credential cannot create pull requests in {project}; install a credential with write access and rerun `wt-server-installer`"
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
                            body: crate::api::attributed_comment(scope, body),
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
                                body: crate::api::attributed_comment(scope, body),
                            },
                        )?;
                    }
                    GithubReviewTarget::PullRequest(id) => {
                        self.graphql
                            .execute_graphql::<GithubAddPullRequestComment>(
                                self.graphql_path,
                                github_add_pull_request_comment::Variables {
                                    id: id.0,
                                    body: crate::api::attributed_comment(scope, body),
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
                    None => Ok(ProviderCommandOutput::CiJobLog(
                        unavailable_job_log(
                            &current,
                            &self.read_check_run_annotations(scope.project, &current.handle)?,
                        ),
                    )),
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
                    "GitHub cannot cancel one Actions job: its API can only cancel the entire workflow run, including sibling jobs; wt-git-hosting refuses to widen `cancel {job}` beyond the selected job"
                )
            }
            ProviderCommand::WaitForReviewOrCiChange => {
                crate::api::wait_for_review_or_ci_change(self, scope)
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
            CliCommand::ShowMrForBranch { branch } => Ok(ProviderCommandOutput::ChangeRequest(
                pull_request_status(self.read_open_pull_request_for_branch(scope.project, branch)?),
            )),
            CliCommand::ShowRun { run } => Ok(ProviderCommandOutput::CiRun(ci_run(
                self.read_workflow_run(scope.project, *run)?,
            ))),
            CliCommand::ShowJob { job } => Ok(ProviderCommandOutput::CiJob(ci_job(
                self.read_workflow_job(scope.project, *job)?,
            ))),
            CliCommand::ListThreads { mr } => Ok(ProviderCommandOutput::ReviewThreads(
                self.read_review_threads(scope.project, *mr)?,
            )),
            CliCommand::ListCi { commit } => {
                let (runs, jobs) = self.list_ci_for_commit(scope.project, commit)?;
                Ok(ProviderCommandOutput::CiRunsAndJobs { runs, jobs })
            }
            CliCommand::ListJobs { run } => Ok(ProviderCommandOutput::CiJobs(
                self.list_run_jobs(scope.project, *run)?,
            )),
            CliCommand::LogJob { job } => {
                let project_scope = ProviderCommandScope {
                    project: scope.project,
                    base: "",
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
            CliCommand::WaitMr {
                mr,
                timeout_seconds,
            } => {
                let deadline = cli_wait_deadline(*timeout_seconds);
                let initial = self.read_pull_request(scope.project, *mr)?;
                if matches!(initial.state.as_str(), "closed" | "merged") {
                    return Ok(ProviderCommandOutput::ChangeRequest(pull_request_status(
                        initial,
                    )));
                }
                loop {
                    if !wait_for_next_cli_poll(deadline) {
                        bail!(
                            "MR {mr} did not change before the wait timeout; last state: {}",
                            initial.state
                        );
                    }
                    let current = self.read_pull_request(scope.project, *mr)?;
                    if current != initial {
                        return Ok(ProviderCommandOutput::ChangeRequest(pull_request_status(
                            current,
                        )));
                    }
                }
            }
            CliCommand::WaitRun {
                run,
                timeout_seconds,
            } => {
                let deadline = cli_wait_deadline(*timeout_seconds);
                loop {
                    let current = self.read_workflow_run(scope.project, *run)?;
                    let output = ci_run(current);
                    if ci_terminal(&output.state) {
                        return Ok(ProviderCommandOutput::CiRun(output));
                    }
                    if !wait_for_next_cli_poll(deadline) {
                        bail!(
                            "CI run {run} did not finish before the wait timeout; last state: {}",
                            output.state
                        );
                    }
                }
            }
            CliCommand::WaitJob {
                job,
                timeout_seconds,
            } => {
                let deadline = cli_wait_deadline(*timeout_seconds);
                loop {
                    let current = self.read_workflow_job(scope.project, *job)?;
                    let output = ci_job(current);
                    if ci_terminal(&output.state) {
                        return Ok(ProviderCommandOutput::CiJob(output));
                    }
                    if !wait_for_next_cli_poll(deadline) {
                        bail!(
                            "CI job {job} did not finish before the wait timeout; last state: {}",
                            output.state
                        );
                    }
                }
            }
            CliCommand::OpenMr { head, base, draft } => {
                if !head.starts_with(scope.prefix) {
                    bail!("open mr must use a {}* head", scope.prefix);
                }
                let encoded: String =
                    url::form_urlencoded::byte_serialize(head.as_bytes()).collect();
                let commit: Commit = self.rest.read_json(&format!(
                    "{}repos/{}/commits/{encoded}",
                    self.rest_prefix, scope.project
                ))?;
                let current = ProviderCommandScope {
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
                match state {
                    ChangeRequestState::Ready => {
                        self.graphql.execute_graphql::<GithubMarkPullRequestReady>(
                            self.graphql_path,
                            github_mark_pull_request_ready::Variables {
                                id: request.node_id,
                            },
                        )?;
                    }
                    ChangeRequestState::Draft => {
                        self.graphql.execute_graphql::<GithubMarkPullRequestDraft>(
                            self.graphql_path,
                            github_mark_pull_request_draft::Variables {
                                id: request.node_id,
                            },
                        )?;
                    }
                    ChangeRequestState::Open | ChangeRequestState::Closed => {
                        self.graphql.execute_graphql::<GithubUpdatePullRequest>(
                            self.graphql_path,
                            github_update_pull_request::Variables {
                                id: request.node_id,
                                title: None,
                                body: None,
                                state: Some(if matches!(state, ChangeRequestState::Open) {
                                    github_update_pull_request::PullRequestUpdateState::OPEN
                                } else {
                                    github_update_pull_request::PullRequestUpdateState::CLOSED
                                }),
                            },
                        )?;
                    }
                }
                Ok(ProviderCommandOutput::ChangeRequest(pull_request_status(
                    self.read_pull_request(scope.project, *mr)?,
                )))
            }
            CliCommand::EditMr { mr, title, body } => {
                let request = self.read_pull_request(scope.project, *mr)?;
                Self::require_writable_pull_request(scope, &request)?;
                self.graphql.execute_graphql::<GithubUpdatePullRequest>(
                    self.graphql_path,
                    github_update_pull_request::Variables {
                        id: request.node_id,
                        title: title.clone(),
                        body: body.clone(),
                        state: None,
                    },
                )?;
                Ok(ProviderCommandOutput::ChangeRequest(pull_request_status(
                    self.read_pull_request(scope.project, *mr)?,
                )))
            }
            CliCommand::CommentMr { mr, body } => {
                let request = self.read_pull_request(scope.project, *mr)?;
                Self::require_writable_pull_request(scope, &request)?;
                self.graphql
                    .execute_graphql::<GithubAddPullRequestComment>(
                        self.graphql_path,
                        github_add_pull_request_comment::Variables {
                            id: request.node_id,
                            body: crate::api::attributed_project_comment(scope, body),
                        },
                    )?;
                Ok(ProviderCommandOutput::Confirmation(
                    "Comment added.".to_owned(),
                ))
            }
            CliCommand::ReplyThread { mr, thread, body } => {
                let request = self.read_pull_request(scope.project, *mr)?;
                Self::require_writable_pull_request(scope, &request)?;
                self.require_review_thread(scope.project, *mr, thread)?;
                self.graphql.execute_graphql::<GithubReplyToReviewThread>(
                    self.graphql_path,
                    github_reply_to_review_thread::Variables {
                        thread: thread.as_str().to_owned(),
                        body: crate::api::attributed_project_comment(scope, body),
                    },
                )?;
                Ok(ProviderCommandOutput::Confirmation(
                    "Reply added.".to_owned(),
                ))
            }
            CliCommand::SetThread {
                mr,
                thread,
                resolved,
            } => {
                let request = self.read_pull_request(scope.project, *mr)?;
                Self::require_writable_pull_request(scope, &request)?;
                self.require_review_thread(scope.project, *mr, thread)?;
                if *resolved {
                    self.graphql.execute_graphql::<GithubResolveReviewThread>(
                        self.graphql_path,
                        github_resolve_review_thread::Variables {
                            thread: thread.as_str().to_owned(),
                        },
                    )?;
                } else {
                    self.graphql.execute_graphql::<GithubReopenReviewThread>(
                        self.graphql_path,
                        github_reopen_review_thread::Variables {
                            thread: thread.as_str().to_owned(),
                        },
                    )?;
                }
                Ok(ProviderCommandOutput::Confirmation(if *resolved {
                    "Thread resolved.".to_owned()
                } else {
                    "Thread reopened.".to_owned()
                }))
            }
            CliCommand::RetryJob { job } | CliCommand::CancelJob { job } => {
                let current = self.read_workflow_job(scope.project, *job)?;
                let run = self.read_workflow_run(scope.project, current.run_id)?;
                Self::require_writable_run(scope, &run)?;
                if matches!(command, CliCommand::CancelJob { .. }) {
                    bail!(
                        "GitHub cannot cancel one job; use a `cancel_run` JSON action for run {}",
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
            CliCommand::ReportAgGitBug { .. }
            | CliCommand::ReportAgGitIssue { .. }
            | CliCommand::SuggestAgGitImprovement { .. }
            | CliCommand::RequestAgGitFeature { .. } => {
                unreachable!("agent Git reports are handled before provider commands")
            }
        }
    }
}

fn unavailable_job_log(job: &CiJob, annotations: &[CheckRunAnnotation]) -> String {
    let mut output = format!(
        "Job: {} ({})\nState: {}\nLog: GitHub did not publish log bytes for this job.\n",
        job.handle, job.name, job.state
    );
    if annotations.is_empty() {
        output.push_str("Diagnostics: GitHub reported no check annotations.\n");
        output.push_str("Next step: ask the user to resolve this provider-side failure.\n");
        return output;
    }

    output.push_str("Diagnostics:\n");
    for annotation in annotations {
        let location = if annotation.start_line == annotation.end_line {
            format!("{}:{}", annotation.path, annotation.start_line)
        } else {
            format!(
                "{}:{}-{}",
                annotation.path, annotation.start_line, annotation.end_line
            )
        };
        let title = annotation
            .title
            .as_deref()
            .filter(|title| !title.is_empty())
            .map(|title| format!(" {title}"))
            .unwrap_or_default();
        output.push_str(&format!(
            "- [{}] {location}{title}\n  {}\n",
            annotation.annotation_level, annotation.message
        ));
        if let Some(details) = annotation
            .raw_details
            .as_deref()
            .filter(|details| !details.is_empty())
        {
            output.push_str(&format!("  {details}\n"));
        }
    }
    output.push_str("Next step: ask the user to resolve this provider-side failure.\n");
    output
}
