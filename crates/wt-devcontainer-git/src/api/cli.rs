use super::*;

impl CliCommand {
    pub(crate) fn parse(args: &[String]) -> Result<Self> {
        let Some((name, rest)) = args.split_first() else {
            bail!("usage: ag-git COMMAND RESOURCE [ID]");
        };
        match name.as_str() {
            "show" => match rest {
                [kind, id] if kind == "mr" => Ok(Self::ShowMr {
                    mr: numeric(id, "MR")?,
                }),
                [kind, id] if kind == "run" => Ok(Self::ShowRun {
                    run: numeric(id, "run")?,
                }),
                [kind, id] if kind == "job" => Ok(Self::ShowJob {
                    job: numeric(id, "job")?,
                }),
                _ => bail!("usage: ag-git show mr|run|job ID"),
            },
            "list" => match rest {
                [items, parent, id] if items == "threads" && parent == "mr" => {
                    Ok(Self::ListThreads {
                        mr: numeric(id, "MR")?,
                    })
                }
                [items, parent, commit] if items == "ci" && parent == "commit" => {
                    validate_commit(commit)?;
                    Ok(Self::ListCi {
                        commit: commit.clone(),
                    })
                }
                [items, parent, id] if items == "jobs" && parent == "run" => Ok(Self::ListJobs {
                    run: numeric(id, "run")?,
                }),
                _ => bail!("usage: ag-git list threads mr ID | ci commit SHA | jobs run ID"),
            },
            "log" => match rest {
                [kind, id] if kind == "job" => Ok(Self::LogJob {
                    job: numeric(id, "job")?,
                }),
                _ => bail!("usage: ag-git log job ID"),
            },
            "wait" => match rest {
                [kind, id] if kind == "mr" => Ok(Self::WaitMr {
                    mr: numeric(id, "MR")?,
                }),
                [kind, id] if kind == "run" => Ok(Self::WaitRun {
                    run: numeric(id, "run")?,
                }),
                [kind, id] if kind == "job" => Ok(Self::WaitJob {
                    job: numeric(id, "job")?,
                }),
                _ => bail!("usage: ag-git wait mr|run|job ID"),
            },
            "open" => parse_open_mr(rest),
            "set" => parse_set(rest),
            "edit" => parse_explicit_edit(rest),
            "comment" => match rest {
                [kind, id, body @ ..] if kind == "mr" => Ok(Self::CommentMr {
                    mr: numeric(id, "MR")?,
                    body: required_text(body, "ag-git comment mr ID TEXT")?,
                }),
                _ => bail!("usage: ag-git comment mr ID TEXT"),
            },
            "reply" => parse_reply(rest),
            "retry" => match rest {
                [kind, id] if kind == "job" => Ok(Self::RetryJob {
                    job: numeric(id, "job")?,
                }),
                _ => bail!("usage: ag-git retry job ID"),
            },
            "cancel" => match rest {
                [kind, id] if kind == "job" => Ok(Self::CancelJob {
                    job: numeric(id, "job")?,
                }),
                [kind, id] if kind == "run" => Ok(Self::CancelRun {
                    run: numeric(id, "run")?,
                }),
                _ => bail!("usage: ag-git cancel job|run ID"),
            },
            _ => bail!("unknown command `{name}`; run `ag-git --help`"),
        }
    }

    pub(super) fn action(&self) -> &'static str {
        match self {
            Self::ShowMr { .. } => "show the merge request",
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
            Self::ShowMr { mr }
            | Self::ListThreads { mr }
            | Self::WaitMr { mr }
            | Self::SetMr { mr, .. }
            | Self::EditMr { mr, .. }
            | Self::CommentMr { mr, .. } => format!("mr {mr}"),
            Self::ReplyThread { mr, thread, .. } | Self::SetThread { mr, thread, .. } => {
                format!("thread {thread} in mr {mr}")
            }
            Self::ShowRun { run }
            | Self::ListJobs { run }
            | Self::WaitRun { run }
            | Self::CancelRun { run } => format!("run {run}"),
            Self::ShowJob { job }
            | Self::LogJob { job }
            | Self::WaitJob { job }
            | Self::RetryJob { job }
            | Self::CancelJob { job } => format!("job {job}"),
            Self::ListCi { commit } => format!("commit {commit}"),
            Self::OpenMr { head, base, .. } => format!("mr {head} -> {base}"),
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
    format!(
        "MR: {}\nState: {}{}\nTitle: {}\nHead: {}\nBase: {}\nURL: {}\n",
        request.handle,
        request.state,
        if request.draft { " (draft)" } else { "" },
        request.title,
        request.head,
        request.base,
        request.url
    )
}

fn render_run(run: &CiRun) -> String {
    format!(
        "Run: {}\nState: {}\nName: {}\nCommit: {}\nRef: {}\nURL: {}\n",
        run.handle,
        run.state,
        run.name,
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
        .map(|run| format!("run {} [{}] {}\n", run.handle, run.state, run.name))
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
    output.push_str("\nUse `ag-git show job ID`, `ag-git log job ID`, or `ag-git wait job ID`.\n");
    output
}

fn required_text(args: &[String], usage: &str) -> Result<String> {
    if args.is_empty() {
        bail!("usage: {usage}");
    }
    Ok(args.join(" "))
}

fn numeric(value: &str, kind: &str) -> Result<u64> {
    value
        .parse()
        .map_err(|_| anyhow::anyhow!("{kind} ID must be a positive integer"))
        .and_then(|id| {
            if id == 0 {
                bail!("{kind} ID must be a positive integer")
            } else {
                Ok(id)
            }
        })
}

fn validate_commit(commit: &str) -> Result<()> {
    if commit.len() < 7 || commit.len() > 64 || !commit.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        bail!("commit must be a 7 to 64 character hexadecimal object ID");
    }
    Ok(())
}

fn parse_open_mr(args: &[String]) -> Result<CliCommand> {
    let [kind, options @ ..] = args else {
        bail!("usage: ag-git open mr --head BRANCH --base BRANCH [--draft]");
    };
    if kind != "mr" {
        bail!("usage: ag-git open mr --head BRANCH --base BRANCH [--draft]");
    }
    let mut head = None;
    let mut base = None;
    let mut draft = false;
    let mut index = 0;
    while index < options.len() {
        match options[index].as_str() {
            "--head" | "--base" => {
                let target = if options[index] == "--head" {
                    &mut head
                } else {
                    &mut base
                };
                index += 1;
                let value = options
                    .get(index)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "usage: ag-git open mr --head BRANCH --base BRANCH [--draft]"
                        )
                    })?;
                if target.replace(value.clone()).is_some() {
                    bail!("open mr option specified more than once");
                }
            }
            "--draft" if !draft => draft = true,
            _ => bail!("usage: ag-git open mr --head BRANCH --base BRANCH [--draft]"),
        }
        index += 1;
    }
    Ok(CliCommand::OpenMr {
        head: head.context("open mr requires --head BRANCH")?,
        base: base.context("open mr requires --base BRANCH")?,
        draft,
    })
}

fn parse_set(args: &[String]) -> Result<CliCommand> {
    match args {
        [kind, id, state] if kind == "mr" => {
            let state = match state.as_str() {
                "ready" => ChangeRequestState::Ready,
                "draft" => ChangeRequestState::Draft,
                "open" => ChangeRequestState::Open,
                "closed" => ChangeRequestState::Closed,
                _ => bail!("MR state must be ready, draft, open, or closed"),
            };
            Ok(CliCommand::SetMr { mr: numeric(id, "MR")?, state })
        }
        [kind, thread, flag, mr, state] if kind == "thread" && flag == "--mr" => {
            let resolved = match state.as_str() {
                "resolved" => true,
                "open" => false,
                _ => bail!("thread state must be resolved or open"),
            };
            Ok(CliCommand::SetThread {
                mr: numeric(mr, "MR")?,
                thread: ReviewThreadHandle::new(thread),
                resolved,
            })
        }
        _ => bail!("usage: ag-git set mr ID ready|draft|open|closed | set thread ID --mr MR_ID resolved|open"),
    }
}

fn parse_explicit_edit(args: &[String]) -> Result<CliCommand> {
    let [kind, id, options @ ..] = args else {
        bail!("usage: ag-git edit mr ID [--title TEXT] [--body TEXT]");
    };
    if kind != "mr" {
        bail!("usage: ag-git edit mr ID [--title TEXT] [--body TEXT]");
    }
    let mut title = None;
    let mut body = None;
    let mut index = 0;
    while index < options.len() {
        let target = match options[index].as_str() {
            "--title" => &mut title,
            "--body" => &mut body,
            _ => bail!("usage: ag-git edit mr ID [--title TEXT] [--body TEXT]"),
        };
        index += 1;
        let value = options
            .get(index)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                anyhow::anyhow!("usage: ag-git edit mr ID [--title TEXT] [--body TEXT]")
            })?;
        if target.replace(value.clone()).is_some() {
            bail!("edit option specified more than once");
        }
        index += 1;
    }
    if title.is_none() && body.is_none() {
        bail!("edit requires --title or --body");
    }
    Ok(CliCommand::EditMr {
        mr: numeric(id, "MR")?,
        title,
        body,
    })
}

fn parse_reply(args: &[String]) -> Result<CliCommand> {
    let [kind, thread, flag, mr, body @ ..] = args else {
        bail!("usage: ag-git reply thread ID --mr MR_ID TEXT");
    };
    if kind != "thread" || flag != "--mr" {
        bail!("usage: ag-git reply thread ID --mr MR_ID TEXT");
    }
    Ok(CliCommand::ReplyThread {
        mr: numeric(mr, "MR")?,
        thread: ReviewThreadHandle::new(thread),
        body: required_text(body, "ag-git reply thread ID --mr MR_ID TEXT")?,
    })
}
