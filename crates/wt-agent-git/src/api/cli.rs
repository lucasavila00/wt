use super::*;

impl CliCommand {
    pub(crate) fn parse(args: &[String]) -> Result<Self> {
        let [json] = args else {
            bail!("ag-git expects exactly one JSON command object; run `ag-git help` for the TypeScript command type");
        };
        let command: Self = serde_json::from_str(json).context(
            "invalid ag-git command JSON; run `ag-git help` for the TypeScript command type",
        )?;
        command.validate()?;
        Ok(command)
    }

    fn validate(&self) -> Result<()> {
        match self {
            Self::ShowMr { mr }
            | Self::ListThreads { mr }
            | Self::WaitMr { mr, .. }
            | Self::SetMr { mr, .. }
            | Self::EditMr { mr, .. }
            | Self::CommentMr { mr, .. }
            | Self::ReplyThread { mr, .. }
            | Self::SetThread { mr, .. } => positive_id(*mr, "MR")?,
            Self::ShowRun { run }
            | Self::ListJobs { run }
            | Self::WaitRun { run, .. }
            | Self::CancelRun { run } => positive_id(*run, "run")?,
            Self::ShowJob { job }
            | Self::LogJob { job }
            | Self::WaitJob { job, .. }
            | Self::RetryJob { job }
            | Self::CancelJob { job } => positive_id(*job, "job")?,
            Self::ShowMrForBranch { branch } => nonempty(branch, "branch")?,
            Self::ListCi { commit } => validate_commit(commit)?,
            Self::OpenMr { head, base, .. } => {
                nonempty(head, "head")?;
                nonempty(base, "base")?;
            }
            Self::ReportAgGitBug { description }
            | Self::ReportAgGitIssue { description }
            | Self::SuggestAgGitImprovement { description }
            | Self::RequestAgGitFeature { description } => {
                nonempty(description.trim(), "description")?
            }
        }
        if let Self::EditMr { title, body, .. } = self {
            if title.is_none() && body.is_none() {
                bail!("edit_mr requires `title` or `body`");
            }
        }
        match self {
            Self::WaitMr {
                timeout_seconds, ..
            }
            | Self::WaitRun {
                timeout_seconds, ..
            }
            | Self::WaitJob {
                timeout_seconds, ..
            } if matches!(timeout_seconds, Some(0)) => {
                bail!("timeout_seconds must be a positive integer");
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn action(&self) -> &'static str {
        match self {
            Self::ShowMr { .. } | Self::ShowMrForBranch { .. } => "show the merge request",
            Self::ShowRun { .. } => "show the CI run",
            Self::ShowJob { .. } => "show the CI job",
            Self::ListThreads { .. } => "list merge request threads",
            Self::ListCi { .. } => "list CI for the commit",
            Self::ListJobs { .. } => "list jobs for the CI run",
            Self::LogJob { .. } => "read the CI job log",
            Self::WaitMr { .. } => "wait for the merge request to change",
            Self::WaitRun { .. } => "wait for the CI run to finish",
            Self::WaitJob { .. } => "wait for the CI job to finish",
            Self::OpenMr { .. } => "open the merge request",
            Self::SetMr { .. } => "set the merge request state",
            Self::EditMr { .. } => "edit the merge request",
            Self::CommentMr { .. } => "comment on the merge request",
            Self::ReplyThread { .. } => "reply to the review thread",
            Self::SetThread { .. } => "set the review thread state",
            Self::RetryJob { .. } => "retry the CI job",
            Self::CancelJob { .. } => "cancel the CI job",
            Self::CancelRun { .. } => "cancel the CI run",
            Self::ReportAgGitBug { .. } => "report an ag-git bug",
            Self::ReportAgGitIssue { .. } => "report an ag-git issue",
            Self::SuggestAgGitImprovement { .. } => "suggest an ag-git improvement",
            Self::RequestAgGitFeature { .. } => "request an ag-git feature",
        }
    }

    pub(super) fn resource(&self) -> String {
        match self {
            Self::ShowMrForBranch { branch } => format!("mr for branch {branch}"),
            Self::ShowMr { mr }
            | Self::ListThreads { mr }
            | Self::WaitMr { mr, .. }
            | Self::SetMr { mr, .. }
            | Self::EditMr { mr, .. }
            | Self::CommentMr { mr, .. } => format!("mr {mr}"),
            Self::ReplyThread { mr, thread, .. } | Self::SetThread { mr, thread, .. } => {
                format!("thread {thread} in mr {mr}")
            }
            Self::ShowRun { run }
            | Self::ListJobs { run }
            | Self::WaitRun { run, .. }
            | Self::CancelRun { run } => format!("run {run}"),
            Self::ShowJob { job }
            | Self::LogJob { job }
            | Self::WaitJob { job, .. }
            | Self::RetryJob { job }
            | Self::CancelJob { job } => format!("job {job}"),
            Self::ListCi { commit } => format!("commit {commit}"),
            Self::OpenMr { head, base, .. } => format!("mr {head} -> {base}"),
            Self::ReportAgGitBug { .. }
            | Self::ReportAgGitIssue { .. }
            | Self::SuggestAgGitImprovement { .. }
            | Self::RequestAgGitFeature { .. } => "ag-git".to_owned(),
        }
    }

    pub(crate) fn agent_git_report(&self) -> Option<(wt_registry::AgentGitReportKind, &str)> {
        match self {
            Self::ReportAgGitBug { description } => {
                Some((wt_registry::AgentGitReportKind::Bug, description))
            }
            Self::ReportAgGitIssue { description } => {
                Some((wt_registry::AgentGitReportKind::Issue, description))
            }
            Self::SuggestAgGitImprovement { description } => {
                Some((wt_registry::AgentGitReportKind::Improvement, description))
            }
            Self::RequestAgGitFeature { description } => {
                Some((wt_registry::AgentGitReportKind::FeatureRequest, description))
            }
            _ => None,
        }
    }
}

pub(crate) fn render_cli_command_output(output: ProviderCommandOutput) -> String {
    match output {
        ProviderCommandOutput::ChangeRequest(request) => render_change_request(&request),
        ProviderCommandOutput::ReviewThreads(threads) => render_threads(&threads),
        ProviderCommandOutput::CiJobs(jobs) => render_jobs(&jobs),
        ProviderCommandOutput::CiRun(run) => render_run(&run),
        ProviderCommandOutput::CiRunsAndJobs { runs, jobs } => render_ci(&runs, &jobs),
        ProviderCommandOutput::CiJob(job) => render_job(&job),
        ProviderCommandOutput::CiJobLog(log) => tail_ci_job_log(log),
        ProviderCommandOutput::Confirmation(message) => format!("{message}\n"),
        ProviderCommandOutput::CurrentStatus(_) => {
            unreachable!("contextual status is not a public CLI result")
        }
    }
}

fn render_change_request(request: &ChangeRequestStatus) -> String {
    let mut output = format!(
        "MR: {}\nState: {}{}\nTitle: {}\nHead: {}\nBase: {}\nURL: {}\n",
        request.handle,
        request.state,
        if request.draft { " (draft)" } else { "" },
        request.title,
        request.head,
        request.base,
        request.url
    );
    match &request.body {
        Some(body) => output.push_str(&format!("Body:\n{body}\n")),
        None => output.push_str("Body: unavailable\n"),
    }
    output
}

fn render_run(run: &CiRun) -> String {
    format!(
        "Run: {}\nState: {}\nName: {}\nTrigger: {}\nCommit: {}\nRef: {}\nURL: {}\n",
        run.handle,
        run.state,
        run.name,
        run.trigger.as_deref().unwrap_or("unknown"),
        run.head,
        run.branch.as_deref().unwrap_or("unknown"),
        run.url.as_deref().unwrap_or("unavailable")
    )
}

fn render_job(job: &CiJob) -> String {
    format!(
        "Job: {}\nRun: {}\nState: {}\nName: {}\nURL: {}\n",
        job.handle,
        job.run.as_deref().unwrap_or("unknown"),
        job.state,
        job.name,
        job.url.as_deref().unwrap_or("unavailable")
    )
}

fn render_ci(runs: &[CiRun], jobs: &[CiJob]) -> String {
    let mut output = runs
        .iter()
        .map(|run| {
            format!(
                "run {} [{}] {} trigger={}\n",
                run.handle,
                run.state,
                run.name,
                run.trigger.as_deref().unwrap_or("unknown")
            )
        })
        .collect::<String>();
    output.push_str(
        &jobs
            .iter()
            .map(|job| {
                format!(
                    "job {} run {} [{}] {}\n",
                    job.handle,
                    job.run.as_deref().unwrap_or("unknown"),
                    job.state,
                    job.name
                )
            })
            .collect::<String>(),
    );
    if output.is_empty() {
        "No CI resources for the commit.\n".to_owned()
    } else {
        output
    }
}

fn tail_ci_job_log(log: String) -> String {
    tail_ci_job_log_at_limit(log, CI_JOB_LOG_TAIL_LIMIT)
}

pub(super) fn tail_ci_job_log_at_limit(log: String, limit: usize) -> String {
    if log.len() <= limit {
        return log;
    }
    let retained = limit - CI_JOB_LOG_TRUNCATION_NOTICE.len();
    let mut start = log.len() - retained;
    while !log.is_char_boundary(start) {
        start += 1;
    }
    format!("{CI_JOB_LOG_TRUNCATION_NOTICE}{}", &log[start..])
}

pub(super) fn render_threads(threads: &[ReviewThread]) -> String {
    if threads.is_empty() {
        return "No review threads.\n".to_owned();
    }
    let mut output = String::new();
    for thread in threads {
        let location = match (&thread.path, thread.line) {
            (Some(path), Some(line)) => format!(" {path}:{line}"),
            (Some(path), None) => format!(" {path}"),
            _ => String::new(),
        };
        output.push_str(&format!(
            "{} [{}]{}\n",
            thread.handle,
            if !thread.resolvable {
                "feedback"
            } else if thread.resolved {
                "resolved"
            } else {
                "open"
            },
            location
        ));
        for comment in &thread.comments {
            output.push_str(&format!("  {}: {}\n", comment.author, comment.body));
            if let Some(url) = &comment.url {
                output.push_str(&format!("  {url}\n"));
            }
        }
    }
    output
}

fn render_jobs(jobs: &[CiJob]) -> String {
    if jobs.is_empty() {
        return "No CI jobs for the current commit.\n".to_owned();
    }
    let mut output = jobs
        .iter()
        .map(|job| {
            let url = job
                .url
                .as_deref()
                .map(|url| format!("\n  {url}"))
                .unwrap_or_default();
            format!("{} [{}] {}{url}\n", job.handle, job.state, job.name)
        })
        .collect::<String>();
    output.push_str(
        "\nUse a listed job ID with the `show_job`, `log_job`, or `wait_job` JSON action.\n",
    );
    output
}

fn positive_id(id: u64, kind: &str) -> Result<()> {
    if id == 0 {
        bail!("{kind} ID must be a positive integer");
    }
    Ok(())
}

fn nonempty(value: &str, name: &str) -> Result<()> {
    if value.is_empty() {
        bail!("{name} must not be empty");
    }
    Ok(())
}

fn validate_commit(commit: &str) -> Result<()> {
    if commit.len() < 7 || commit.len() > 64 || !commit.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        bail!("commit must be a 7 to 64 character hexadecimal object ID");
    }
    Ok(())
}
