use super::*;
use crate::api::{cli_wait_deadline, wait_for_next_cli_poll};

impl GitProviderApi for GitlabApi {
    fn verify_repository_access(&self, project: &str, base: &str) -> Result<()> {
        let data = self.http.execute_graphql::<GitlabReadMergeRequest>(
            "api/graphql",
            gitlab_read_merge_request::Variables {
                project: project.to_owned(),
                branch: "__wt_access_check__".to_owned(),
                branches: Some(vec!["__wt_access_check__".to_owned()]),
                bases: Some(vec![base.to_owned()]),
            },
        )?;
        data.current_user
            .context("GitLab API credential did not identify a user")?;
        let project_data = data
            .project
            .with_context(|| format!("GitLab project {project} was not found"))?;
        if project_data.user_permissions.create_merge_request_in {
            Ok(())
        } else {
            bail!(
                "GitLab API credential cannot create merge requests in {project}; install a credential with permission to create merge requests and rerun `wt-server-installer`"
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
                        scope.project,
                        snapshot
                            .merge_request_number
                            .context("merge request has no number")?,
                        true,
                    )?;
                }
                self.read_refreshed_change_request(scope)
            }
            ProviderCommand::MarkChangeRequestReady | ProviderCommand::MarkChangeRequestDraft => {
                let merge_request_number = self
                    .require_change_request(scope)?
                    .merge_request_number
                    .context("merge request has no number")?;
                set_change_request_draft(
                    &self.http,
                    scope.project,
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
                        body: crate::api::attributed_comment(scope, body),
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
                    .context("merge request has no number")?;
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
                let discussion = Self::discussion_id(&snapshot.discussions, thread)?;
                let id = snapshot
                    .merge_request_id
                    .context("merge request has no ID")?;
                let data = self.http.execute_graphql::<GitlabReplyToDiscussion>(
                    "api/graphql",
                    gitlab_reply_to_discussion::Variables {
                        id: NoteableID(id.0),
                        discussion: DiscussionID(discussion.0),
                        body: crate::api::attributed_comment(scope, body),
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
                let discussion = Self::discussion_id(&snapshot.discussions, thread)?;
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
                    .context("merge request has no number")?;
                Ok(ProviderCommandOutput::CiJobs(
                    self.read_ci_jobs(scope, &merge_request_number)?,
                ))
            }
            ProviderCommand::ReadCiJobLog { job } => {
                let job_id = job.as_str().parse::<u64>().map_err(|_| {
                    anyhow::anyhow!(
                        "`{job}` is not a numeric GitLab CI job ID; use the job ID from its GitLab CI URL"
                    )
                })?;
                Ok(ProviderCommandOutput::CiJobLog(self.http.read_text(
                    &format!(
                        "api/v4/projects/{}/jobs/{job_id}/trace",
                        encoded_project(scope.project)
                    ),
                )?))
            }
            ProviderCommand::RetryCiJob { job } => {
                self.require_ci_job(scope, job)?;
                self.http.post_without_body(&format!(
                    "api/v4/projects/{}/jobs/{job}/retry",
                    encoded_project(scope.project)
                ))?;
                Ok(ProviderCommandOutput::Confirmation(format!(
                    "Retry requested for job {job}."
                )))
            }
            ProviderCommand::CancelCiJob { job } => {
                self.require_ci_job(scope, job)?;
                self.http.post_without_body(&format!(
                    "api/v4/projects/{}/jobs/{job}/cancel",
                    encoded_project(scope.project)
                ))?;
                Ok(ProviderCommandOutput::Confirmation(format!(
                    "Cancellation requested for job {job}."
                )))
            }
            ProviderCommand::WaitForReviewOrCiChange => {
                crate::api::wait_for_review_or_ci_change(self, scope)
            }
            ProviderCommand::CloseChangeRequest | ProviderCommand::ReopenChangeRequest => {
                let merge_request_number = self
                    .require_change_request(scope)?
                    .merge_request_number
                    .context("merge request has no number")?;
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

    fn execute_cli_command(
        &self,
        scope: &ProviderProjectScope<'_>,
        command: &CliCommand,
    ) -> Result<ProviderCommandOutput> {
        match command {
            CliCommand::ShowMr { mr } => Ok(ProviderCommandOutput::ChangeRequest(
                merge_request_status(self.read_merge_request(scope.project, *mr)?),
            )),
            CliCommand::ShowMrForBranch { branch } => {
                Ok(ProviderCommandOutput::ChangeRequest(merge_request_status(
                    self.read_open_merge_request_for_branch(scope.project, branch)?,
                )))
            }
            CliCommand::ShowRun { run } => Ok(ProviderCommandOutput::CiRun(gitlab_run(
                self.read_pipeline(scope.project, *run)?,
            ))),
            CliCommand::ShowJob { job } => Ok(ProviderCommandOutput::CiJob(gitlab_job(
                self.read_job(scope.project, *job)?,
            ))),
            CliCommand::ListThreads { mr } => Ok(ProviderCommandOutput::ReviewThreads(
                self.read_merge_request_by_iid(scope.project, *mr)?.threads,
            )),
            CliCommand::ListCi { commit } => {
                let (runs, jobs) = self.list_ci_for_commit(scope.project, commit)?;
                Ok(ProviderCommandOutput::CiRunsAndJobs { runs, jobs })
            }
            CliCommand::ListJobs { run } => Ok(ProviderCommandOutput::CiJobs(
                self.list_pipeline_jobs(scope.project, *run)?,
            )),
            CliCommand::LogJob { job } => Ok(ProviderCommandOutput::CiJobLog(
                self.http.read_text(&format!(
                    "api/v4/projects/{}/jobs/{job}/trace",
                    encoded_project(scope.project)
                ))?,
            )),
            CliCommand::WaitMr {
                mr,
                timeout_seconds,
            } => {
                let deadline = cli_wait_deadline(*timeout_seconds);
                let initial = self.read_merge_request(scope.project, *mr)?;
                if matches!(initial.state.as_str(), "closed" | "merged") {
                    return Ok(ProviderCommandOutput::ChangeRequest(merge_request_status(
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
                    let current = self.read_merge_request(scope.project, *mr)?;
                    if current != initial {
                        return Ok(ProviderCommandOutput::ChangeRequest(merge_request_status(
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
                    let output = gitlab_run(self.read_pipeline(scope.project, *run)?);
                    if gitlab_ci_terminal(&output.state) {
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
                    let output = gitlab_job(self.read_job(scope.project, *job)?);
                    if gitlab_ci_terminal(&output.state) {
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
                let commit: Commit = self.http.read_json(&format!(
                    "api/v4/projects/{}/repository/commits/{encoded}",
                    encoded_project(scope.project)
                ))?;
                let current = ProviderCommandScope {
                    project: scope.project,
                    base,
                    prefix: scope.prefix,
                    branch: head,
                    head: &commit.id,
                };
                self.execute_command(
                    &current,
                    &ProviderCommand::OpenChangeRequest { draft: *draft },
                )
            }
            CliCommand::SetMr { mr, state } => {
                let request = self.read_merge_request(scope.project, *mr)?;
                Self::require_writable_merge_request(scope, &request)?;
                match state {
                    ChangeRequestState::Ready | ChangeRequestState::Draft => {
                        set_change_request_draft(
                            &self.http,
                            scope.project,
                            GitlabMergeRequestNumber(mr.to_string()),
                            matches!(state, ChangeRequestState::Draft),
                        )?;
                    }
                    ChangeRequestState::Open | ChangeRequestState::Closed => {
                        let data = self.http.execute_graphql::<GitlabUpdateMergeRequest>(
                            "api/graphql",
                            gitlab_update_merge_request::Variables {
                                project: scope.project.to_owned(),
                                iid: mr.to_string(),
                                title: None,
                                description: None,
                                state: Some(if matches!(state, ChangeRequestState::Open) {
                                    gitlab_update_merge_request::MergeRequestNewState::OPEN
                                } else {
                                    gitlab_update_merge_request::MergeRequestNewState::CLOSED
                                }),
                            },
                        )?;
                        ensure_errors(
                            data.merge_request_update
                                .context("GitLab returned no result")?
                                .errors,
                        )?;
                    }
                }
                Ok(ProviderCommandOutput::ChangeRequest(merge_request_status(
                    self.read_merge_request(scope.project, *mr)?,
                )))
            }
            CliCommand::EditMr { mr, title, body } => {
                let request = self.read_merge_request(scope.project, *mr)?;
                Self::require_writable_merge_request(scope, &request)?;
                let data = self.http.execute_graphql::<GitlabUpdateMergeRequest>(
                    "api/graphql",
                    gitlab_update_merge_request::Variables {
                        project: scope.project.to_owned(),
                        iid: mr.to_string(),
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
                Ok(ProviderCommandOutput::ChangeRequest(merge_request_status(
                    self.read_merge_request(scope.project, *mr)?,
                )))
            }
            CliCommand::CommentMr { mr, body } => {
                let request = self.read_merge_request(scope.project, *mr)?;
                Self::require_writable_merge_request(scope, &request)?;
                let direct = self.read_merge_request_by_iid(scope.project, *mr)?;
                let data = self.http.execute_graphql::<GitlabAddMergeRequestComment>(
                    "api/graphql",
                    gitlab_add_merge_request_comment::Variables {
                        id: NoteableID(direct.id.0),
                        body: crate::api::attributed_project_comment(scope, body),
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
            CliCommand::ReplyThread { mr, thread, body } => {
                let request = self.read_merge_request(scope.project, *mr)?;
                Self::require_writable_merge_request(scope, &request)?;
                let direct = self.read_merge_request_by_iid(scope.project, *mr)?;
                let discussion = Self::discussion_id(&direct.discussions, thread)?;
                let data = self.http.execute_graphql::<GitlabReplyToDiscussion>(
                    "api/graphql",
                    gitlab_reply_to_discussion::Variables {
                        id: NoteableID(direct.id.0),
                        discussion: DiscussionID(discussion.0),
                        body: crate::api::attributed_project_comment(scope, body),
                        head: direct
                            .head
                            .context("GitLab merge request has no head commit")?,
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
            CliCommand::SetThread {
                mr,
                thread,
                resolved,
            } => {
                let request = self.read_merge_request(scope.project, *mr)?;
                Self::require_writable_merge_request(scope, &request)?;
                let direct = self.read_merge_request_by_iid(scope.project, *mr)?;
                let discussion = Self::discussion_id(&direct.discussions, thread)?;
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
            CliCommand::RetryJob { job } | CliCommand::CancelJob { job } => {
                let current = self.read_job(scope.project, *job)?;
                Self::require_writable_ref(scope, current.reference.as_deref())?;
                let action = if matches!(command, CliCommand::RetryJob { .. }) {
                    "retry"
                } else {
                    "cancel"
                };
                self.http.post_without_body(&format!(
                    "api/v4/projects/{}/jobs/{job}/{action}",
                    encoded_project(scope.project)
                ))?;
                Ok(ProviderCommandOutput::Confirmation(format!(
                    "{} requested for job {job}.",
                    if action == "retry" {
                        "Retry"
                    } else {
                        "Cancellation"
                    }
                )))
            }
            CliCommand::CancelRun { run } => {
                let current = self.read_pipeline(scope.project, *run)?;
                Self::require_writable_ref(scope, current.reference.as_deref())?;
                self.http.post_without_body(&format!(
                    "api/v4/projects/{}/pipelines/{run}/cancel",
                    encoded_project(scope.project)
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
