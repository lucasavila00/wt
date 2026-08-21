use super::*;

impl WtToolsCommand {
    pub fn parse(args: &[String]) -> Result<Self> {
        let [json] = args else {
            bail!("wt-tools expects exactly one JSON command object; run `wt-tools help` for the TypeScript command type");
        };
        let parsed: Self = serde_json::from_str(json).context(
            "invalid wt-tools command JSON; run `wt-tools help` for the TypeScript command type",
        )?;
        match &parsed {
            Self::GitHosting { target, command } => {
                nonempty(&target.repository, "repository")?;
                command.validate()?;
            }
            Self::Feedback { command } => command.validate()?,
        }
        Ok(parsed)
    }
}

impl GitHostingCommand {
    fn validate(&self) -> Result<()> {
        match self {
            Self::ShowMr { mr, .. }
            | Self::ListThreads { mr, .. }
            | Self::WaitMr { mr, .. }
            | Self::SetMr { mr, .. }
            | Self::EditMr { mr, .. }
            | Self::CommentMr { mr, .. }
            | Self::ReplyThread { mr, .. }
            | Self::SetThread { mr, .. } => nonempty(mr, "MR ID")?,
            Self::ShowRun { run }
            | Self::ListJobs { run }
            | Self::WaitRun { run, .. }
            | Self::CancelRun { run } => nonempty(run, "run ID")?,
            Self::ShowJob { job }
            | Self::LogJob { job }
            | Self::WaitJob { job, .. }
            | Self::RetryJob { job }
            | Self::CancelJob { job } => nonempty(job, "job ID")?,
            Self::ShowMrForBranch { branch } => nonempty(branch, "branch")?,
            Self::ListCi { commit } => validate_commit(commit)?,
            Self::OpenMr { head, base, .. } => {
                nonempty(head, "head")?;
                nonempty(base, "base")?;
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
        }
    }

    pub(super) fn resource(&self) -> String {
        match self {
            Self::ShowMrForBranch { branch, .. } => format!("mr for branch {branch}"),
            Self::ShowMr { mr, .. }
            | Self::ListThreads { mr, .. }
            | Self::WaitMr { mr, .. }
            | Self::SetMr { mr, .. }
            | Self::EditMr { mr, .. }
            | Self::CommentMr { mr, .. } => format!("mr {mr}"),
            Self::ReplyThread { mr, thread, .. } | Self::SetThread { mr, thread, .. } => {
                format!("thread {thread} in mr {mr}")
            }
            Self::ShowRun { run, .. }
            | Self::ListJobs { run, .. }
            | Self::WaitRun { run, .. }
            | Self::CancelRun { run, .. } => format!("run {run}"),
            Self::ShowJob { job, .. }
            | Self::LogJob { job, .. }
            | Self::WaitJob { job, .. }
            | Self::RetryJob { job, .. }
            | Self::CancelJob { job, .. } => format!("job {job}"),
            Self::ListCi { commit, .. } => format!("commit {commit}"),
            Self::OpenMr { head, base, .. } => format!("mr {head} -> {base}"),
        }
    }
}

impl WtToolsFeedbackCommand {
    fn validate(&self) -> Result<()> {
        let description = match self {
            Self::ReportWtToolBug { description }
            | Self::ReportWtToolIssue { description }
            | Self::SuggestWtToolImprovement { description }
            | Self::RequestWtToolFeature { description } => description,
        };
        nonempty(description.trim(), "description")
    }

    pub fn wt_tool_report(&self) -> (wt_workload_registry::AgentToolReportKind, &str) {
        match self {
            Self::ReportWtToolBug { description } => {
                (wt_workload_registry::AgentToolReportKind::Bug, description)
            }
            Self::ReportWtToolIssue { description } => (
                wt_workload_registry::AgentToolReportKind::Issue,
                description,
            ),
            Self::SuggestWtToolImprovement { description } => (
                wt_workload_registry::AgentToolReportKind::Improvement,
                description,
            ),
            Self::RequestWtToolFeature { description } => (
                wt_workload_registry::AgentToolReportKind::FeatureRequest,
                description,
            ),
        }
    }
}

pub fn render_cli_command_output(output: ProviderCommandOutput) -> String {
    let output = match output {
        ProviderCommandOutput::CiJobLog { log, truncated } => ProviderCommandOutput::CiJobLog {
            log: if truncated {
                format!("{CI_JOB_LOG_TRUNCATION_NOTICE}{log}")
            } else {
                log
            },
            truncated,
        },
        output => output,
    };
    let mut message = serde_json::to_string(&output).expect("provider command output serializes");
    message.push('\n');
    message
}

pub fn render_cli_confirmation(message: impl Into<String>) -> String {
    render_cli_command_output(ProviderCommandOutput::Confirmation(message.into()))
}

pub(super) fn parse_resource_id(id: &str, kind: &str) -> Result<u64> {
    let id = id
        .parse::<u64>()
        .with_context(|| format!("{kind} ID must be a positive integer string"))?;
    if id == 0 {
        bail!("{kind} ID must be a positive integer");
    }
    Ok(id)
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
