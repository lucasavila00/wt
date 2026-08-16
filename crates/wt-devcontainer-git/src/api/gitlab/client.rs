mod provider;

use super::*;

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

    pub(crate) fn with_base_url(base_url: String, token: &str) -> Result<Self> {
        Ok(Self {
            http: ProviderHttpClient::new(base_url, token, ProviderAuthentication::Gitlab)?,
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
                branch: scope.branch.to_owned(),
                branches: Some(vec![scope.branch.to_owned()]),
                bases: Some(vec![scope.base.to_owned()]),
            },
        )?;
        let project = data
            .project
            .with_context(|| format!("GitLab project {} was not found", scope.project))?;
        let remote_head = project
            .repository
            .and_then(|repository| repository.commit)
            .map(|commit| commit.sha);
        let merge_requests = project
            .merge_requests
            .context("GitLab returned no merge request connection")?;
        if merge_requests.page_info.has_next_page {
            bail!(
                "GitLab returned more than 100 merge requests for this branch; refusing to choose one from an incomplete result"
            );
        }
        let mut nodes = merge_requests
            .nodes
            .unwrap_or_default()
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        let node = nodes
            .iter()
            .position(|request| request.diff_head_sha.as_deref() == Some(scope.head))
            .map(|index| nodes.remove(index));
        if remote_head.as_deref() != Some(scope.head) {
            let provider_head = remote_head.as_deref().unwrap_or("missing");
            bail!(
                "the provider branch is at commit {provider_head}, but this checkout is at {}; push the branch first",
                scope.head
            );
        }
        let Some(node) = node else {
            return Ok(GitlabChangeRequestSnapshot {
                merge_request_id: None,
                merge_request_number: None,
                request: None,
                discussions: Vec::new(),
            });
        };
        if node.discussions.page_info.has_next_page {
            bail!(
                "GitLab returned more than 100 review discussions; refusing to assign handles from an incomplete result"
            );
        }
        let mut discussions = Vec::new();
        let threads = node
            .discussions
            .nodes
            .unwrap_or_default()
            .into_iter()
            .flatten()
            .filter(|thread| thread.resolvable)
            .map(|thread| {
                if thread.notes.page_info.has_next_page {
                    bail!(
                        "GitLab returned more than 100 comments in one review discussion; refusing to show an incomplete thread"
                    );
                }
                let discussion_id = GitlabDiscussionId(thread.id.0);
                let handle = review_thread_handle(&discussion_id);
                discussions.push((handle.clone(), discussion_id));
                let notes = thread
                    .notes
                    .nodes
                    .unwrap_or_default()
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>();
                let position = notes.iter().find_map(|note| note.position.as_ref());
                Ok(ReviewThread {
                    handle,
                    resolvable: true,
                    resolved: thread.resolved,
                    path: position.map(|position| position.file_path.clone()),
                    line: position
                        .and_then(|position| position.new_line.or(position.old_line))
                        .and_then(|line| u64::try_from(line).ok()),
                    comments: notes
                        .into_iter()
                        .map(|note| ReviewComment {
                            author: note
                                .author
                                .map(|author| author.username)
                                .unwrap_or_else(|| "unknown".to_owned()),
                            body: note.body,
                            url: note.url,
                        })
                        .collect(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
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
            discussions,
        })
    }

    fn require_change_request(
        &self,
        scope: &ProviderCommandScope<'_>,
    ) -> Result<GitlabChangeRequestSnapshot> {
        let snapshot = self.read_change_request_snapshot(scope, false)?;
        if snapshot.request.is_none() {
            bail!("the explicitly identified branch has no merge request");
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
        discussions: &[(ReviewThreadHandle, GitlabDiscussionId)],
        handle: &ReviewThreadHandle,
    ) -> Result<GitlabDiscussionId> {
        let matches = discussions
            .iter()
            .filter(|(candidate, _)| candidate == handle)
            .map(|(_, id)| id)
            .collect::<Vec<_>>();
        match matches.as_slice() {
            [id] => Ok((*id).clone()),
            [] => bail!(
                "review thread `{handle}` was not found; run a `list_threads` JSON action for the MR and use its provider ID"
            ),
            _ => bail!(
                "review thread ID `{handle}` is ambiguous; no thread was changed"
            ),
        }
    }

    fn read_ci_jobs(
        &self,
        scope: &ProviderCommandScope<'_>,
        merge_request_number: &GitlabMergeRequestNumber,
    ) -> Result<Vec<CiJob>> {
        let project = encoded_project(scope.project);
        let details: MergeRequestDetails = self.http.read_json(&format!(
            "api/v4/projects/{project}/merge_requests/{merge_request_number}"
        ))?;
        if details.sha != scope.head {
            bail!(
                "the merge request moved to commit {}, but this checkout is at {}; push or update the checkout before inspecting CI",
                details.sha,
                scope.head
            );
        }
        let Some(pipeline) = details.head_pipeline else {
            return Ok(Vec::new());
        };
        let response: Vec<PipelineJob> = self.http.read_json(&format!(
            "api/v4/projects/{project}/pipelines/{}/jobs?include_retried=false&per_page=100",
            pipeline.id
        ))?;
        if response.len() == 100 {
            bail!("GitLab returned 100 CI jobs; refusing to report a possibly truncated pipeline");
        }
        let mut jobs = response
            .into_iter()
            .map(|job| CiJob {
                handle: CiJobHandle::new(job.id.to_string()),
                run: job
                    .pipeline
                    .as_ref()
                    .map(|pipeline| pipeline.id.to_string()),
                name: job.name,
                state: normalized_ci_state(job.status),
                url: job.web_url,
            })
            .collect::<Vec<_>>();
        let pipeline_state = normalized_ci_state(pipeline.status);
        let pipeline_state_is_missing =
            pipeline_state != "success" && !jobs.iter().any(|job| job.state == pipeline_state);
        if jobs.is_empty() || pipeline_state_is_missing || pipeline.yaml_errors.is_some() {
            jobs.insert(
                0,
                CiJob {
                    handle: CiJobHandle::new(format!("pipeline-{}", pipeline.id)),
                    run: Some(pipeline.id.to_string()),
                    name: pipeline
                        .yaml_errors
                        .map(|error| format!("pipeline configuration: {error}"))
                        .unwrap_or_else(|| "pipeline".to_owned()),
                    state: pipeline_state,
                    url: pipeline.web_url,
                },
            );
        }
        Ok(jobs)
    }

    fn require_ci_job(&self, scope: &ProviderCommandScope<'_>, handle: &CiJobHandle) -> Result<()> {
        if handle.as_str().starts_with("pipeline-") {
            bail!(
                "`{handle}` is the overall pipeline status, not a job; run a `list_ci` JSON action and choose a job handle"
            );
        }
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
            bail!(
                "CI job `{handle}` does not belong to the current commit; run a `list_ci` JSON action and use a current job handle"
            )
        }
    }

    fn read_merge_request(&self, project: &str, mr: u64) -> Result<MergeRequest> {
        self.http.read_json(&format!(
            "api/v4/projects/{}/merge_requests/{mr}",
            encoded_project(project)
        ))
    }

    fn read_open_merge_request_for_branch(
        &self,
        project: &str,
        base: &str,
        branch: &str,
    ) -> Result<MergeRequest> {
        let source = url::form_urlencoded::byte_serialize(branch.as_bytes()).collect::<String>();
        let target = url::form_urlencoded::byte_serialize(base.as_bytes()).collect::<String>();
        let mut requests: Vec<MergeRequest> = self.http.read_json(&format!(
            "api/v4/projects/{}/merge_requests?state=opened&source_branch={source}&target_branch={target}&per_page=100",
            encoded_project(project)
        ))?;
        requests.retain(|request| {
            request.state == "opened"
                && request.source_branch == branch
                && request.target_branch == base
        });
        match requests.len() {
            0 => bail!("no open merge request from branch `{branch}` to `{base}`"),
            1 => Ok(requests.pop().expect("one merge request remains")),
            count => bail!(
                "GitLab returned {count} open merge requests from branch `{branch}` to `{base}`; refusing to choose one"
            ),
        }
    }

    fn read_merge_request_by_iid(
        &self,
        project: &str,
        mr: u64,
    ) -> Result<GitlabDirectMergeRequest> {
        let data = self.http.execute_graphql::<GitlabReadMergeRequestByIid>(
            "api/graphql",
            gitlab_read_merge_request_by_iid::Variables {
                project: project.to_owned(),
                iid: mr.to_string(),
            },
        )?;
        let node = data
            .project
            .with_context(|| format!("GitLab project {project} was not found"))?
            .merge_request
            .with_context(|| format!("GitLab merge request {mr} was not found"))?;
        if node.discussions.page_info.has_next_page {
            bail!(
                "GitLab returned more than 100 review discussions; refusing to show an incomplete result"
            );
        }
        let mut discussions = Vec::new();
        let threads = node
            .discussions
            .nodes
            .unwrap_or_default()
            .into_iter()
            .flatten()
            .filter(|thread| thread.resolvable)
            .map(|thread| {
                if thread.notes.page_info.has_next_page {
                    bail!(
                        "GitLab returned more than 100 comments in one review discussion; refusing to show an incomplete thread"
                    );
                }
                let discussion_id = GitlabDiscussionId(thread.id.0);
                let handle = review_thread_handle(&discussion_id);
                discussions.push((handle.clone(), discussion_id));
                let notes = thread
                    .notes
                    .nodes
                    .unwrap_or_default()
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>();
                let position = notes.iter().find_map(|note| note.position.as_ref());
                Ok(ReviewThread {
                    handle,
                    resolvable: true,
                    resolved: thread.resolved,
                    path: position.map(|position| position.file_path.clone()),
                    line: position
                        .and_then(|position| position.new_line.or(position.old_line))
                        .and_then(|line| u64::try_from(line).ok()),
                    comments: notes
                        .into_iter()
                        .map(|note| ReviewComment {
                            author: note
                                .author
                                .map(|author| author.username)
                                .unwrap_or_else(|| "unknown".to_owned()),
                            body: note.body,
                            url: note.url,
                        })
                        .collect(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(GitlabDirectMergeRequest {
            id: GitlabMergeRequestId(node.id),
            head: node.diff_head_sha,
            threads,
            discussions,
        })
    }

    fn read_pipeline(&self, project: &str, run: u64) -> Result<Pipeline> {
        self.http.read_json(&format!(
            "api/v4/projects/{}/pipelines/{run}",
            encoded_project(project)
        ))
    }

    fn read_job(&self, project: &str, job: u64) -> Result<PipelineJob> {
        self.http.read_json(&format!(
            "api/v4/projects/{}/jobs/{job}",
            encoded_project(project)
        ))
    }

    pub(super) fn require_writable_merge_request(
        scope: &ProviderProjectScope<'_>,
        request: &MergeRequest,
    ) -> Result<()> {
        if !request.source_branch.starts_with(scope.prefix) || request.target_branch != scope.base {
            bail!(
                "MR {} is outside the writable {}* -> {} scope",
                request.iid,
                scope.prefix,
                scope.base
            );
        }
        Ok(())
    }

    fn require_writable_ref(
        scope: &ProviderProjectScope<'_>,
        reference: Option<&str>,
    ) -> Result<()> {
        if !reference.is_some_and(|reference| reference.starts_with(scope.prefix)) {
            bail!("CI resource is not for a writable {}* ref", scope.prefix);
        }
        Ok(())
    }

    fn list_pipeline_jobs(&self, project: &str, run: u64) -> Result<Vec<CiJob>> {
        let mut page = 1;
        let mut result = Vec::new();
        loop {
            let page_suffix = if page == 1 {
                String::new()
            } else {
                format!("&page={page}")
            };
            let jobs: Vec<PipelineJob> = self.http.read_json(&format!(
                "api/v4/projects/{}/pipelines/{run}/jobs?include_retried=false&per_page=100{page_suffix}",
                encoded_project(project)
            ))?;
            let complete = jobs.len() < 100;
            result.extend(jobs.into_iter().map(gitlab_job));
            if complete {
                return Ok(result);
            }
            page += 1;
        }
    }

    fn list_ci_for_commit(&self, project: &str, commit: &str) -> Result<(Vec<CiRun>, Vec<CiJob>)> {
        let mut page = 1;
        let mut pipelines = Vec::new();
        loop {
            let page_suffix = if page == 1 {
                String::new()
            } else {
                format!("&page={page}")
            };
            let response: Vec<Pipeline> = self.http.read_json(&format!(
                "api/v4/projects/{}/pipelines?sha={commit}&per_page=100{page_suffix}",
                encoded_project(project)
            ))?;
            let complete = response.len() < 100;
            pipelines.extend(response);
            if complete {
                break;
            }
            page += 1;
        }
        let mut runs = Vec::new();
        let mut jobs = Vec::new();
        for pipeline in pipelines {
            jobs.extend(self.list_pipeline_jobs(project, pipeline.id)?);
            runs.push(gitlab_run(pipeline));
        }
        Ok((runs, jobs))
    }
}
