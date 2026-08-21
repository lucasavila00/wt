use super::*;

const CLI_JSON_OUTPUT_LINE_LIMIT: usize = 1_000;
const CLI_COMMAND_CASES: &[(&str, &str)] = &[
    ("show_mr", r#"{"action":"show_mr","mr":7}"#),
    (
        "show_mr_for_branch",
        r#"{"action":"show_mr_for_branch","branch":"wt/fix"}"#,
    ),
    ("show_run", r#"{"action":"show_run","run":91}"#),
    ("show_job", r#"{"action":"show_job","job":44}"#),
    ("list_threads", r#"{"action":"list_threads","mr":7}"#),
    ("list_ci", r#"{"action":"list_ci","commit":"abc1234"}"#),
    ("list_jobs", r#"{"action":"list_jobs","run":91}"#),
    ("log_job", r#"{"action":"log_job","job":44}"#),
    ("wait_mr", r#"{"action":"wait_mr","mr":7}"#),
    ("wait_run", r#"{"action":"wait_run","run":91}"#),
    ("wait_job", r#"{"action":"wait_job","job":44}"#),
    (
        "open_mr",
        r#"{"action":"open_mr","head":"wt/fix","base":"main"}"#,
    ),
    ("set_mr", r#"{"action":"set_mr","mr":7,"state":"ready"}"#),
    (
        "edit_mr",
        r#"{"action":"edit_mr","mr":7,"title":"Fix login"}"#,
    ),
    (
        "comment_mr",
        r#"{"action":"comment_mr","mr":7,"body":"Done"}"#,
    ),
    (
        "reply_thread",
        r#"{"action":"reply_thread","mr":7,"thread":"T1","body":"Done"}"#,
    ),
    (
        "set_thread",
        r#"{"action":"set_thread","mr":7,"thread":"T1","resolved":true}"#,
    ),
    ("retry_job", r#"{"action":"retry_job","job":44}"#),
    ("cancel_job", r#"{"action":"cancel_job","job":44}"#),
    ("cancel_run", r#"{"action":"cancel_run","run":91}"#),
    (
        "report_wt_tool_bug",
        r#"{"action":"report_wt_tool_bug","description":"build failed"}"#,
    ),
    (
        "report_wt_tool_issue",
        r#"{"action":"report_wt_tool_issue","description":"output is unclear"}"#,
    ),
    (
        "suggest_wt_tool_improvement",
        r#"{"action":"suggest_wt_tool_improvement","description":"show progress"}"#,
    ),
    (
        "request_wt_tool_feature",
        r#"{"action":"request_wt_tool_feature","description":"add search"}"#,
    ),
];

#[test]
fn every_cli_command_has_bounded_snapshot_json_output() {
    for &(name, json) in CLI_COMMAND_CASES {
        let command = CliCommand::parse(&[json.to_owned()]).unwrap();
        let rendered = render_cli_command_output(representative_output(command));
        let value: serde_json::Value = serde_json::from_str(&rendered).unwrap();
        let normalized = serde_json::to_string_pretty(&value).unwrap();
        let lines = normalized.lines().count();
        assert!(
            lines <= CLI_JSON_OUTPUT_LINE_LIMIT,
            "{name} output has {lines} normalized JSON lines; limit is {CLI_JSON_OUTPUT_LINE_LIMIT}"
        );
        insta::assert_snapshot!(format!("cli_json_output__{name}"), normalized);
    }
}

#[test]
fn ci_job_logs_keep_only_a_bounded_tail() {
    let output = tail_ci_job_log_at_limit(
        "012345678901234567890123456789αβγδεζηθικλμνξ".to_owned(),
        48,
    );

    insta::assert_snapshot!(output, @r###"
    [earlier CI log output omitted]
    ηθικλμνξ
    "###);
}

#[test]
fn command_parser_accepts_only_valid_json_objects() {
    for &(_, json) in CLI_COMMAND_CASES {
        CliCommand::parse(&[json.to_owned()]).unwrap();
    }
    assert_eq!(
        CliCommand::parse(&[r#"{"action":"wait_job","job":42}"#.into()]).unwrap(),
        CliCommand::WaitJob {
            job: 42,
            timeout_seconds: None,
        }
    );
    assert!(
        CliCommand::parse(&[r#"{"action":"wait_run","run":91,"timeout_seconds":0}"#.into()])
            .is_err()
    );
    assert_eq!(
        CliCommand::parse(&[r#"{"action":"show_mr_for_branch","branch":"wt/fix"}"#.into()])
            .unwrap(),
        CliCommand::ShowMrForBranch {
            branch: "wt/fix".to_owned(),
        }
    );
    assert_eq!(
        CliCommand::parse(&[
            r#"{"action":"open_mr","head":"wt/fix","base":"main","draft":true}"#.into()
        ])
        .unwrap(),
        CliCommand::OpenMr {
            head: "wt/fix".to_owned(),
            base: "main".to_owned(),
            draft: true,
        }
    );
    assert!(CliCommand::parse(&[]).is_err());
    assert!(CliCommand::parse(&["show".into(), "mr".into(), "7".into()]).is_err());
    assert!(CliCommand::parse(&[r#"{"action":"show_mr","mr":0}"#.into()]).is_err());
    assert!(CliCommand::parse(&[r#"{"action":"show_mr","mr":7,"extra":true}"#.into()]).is_err());
    assert!(CliCommand::parse(&[r#"{"action":"edit_mr","mr":7}"#.into()]).is_err());
    assert!(
        CliCommand::parse(&[r#"{"action":"report_wt_tool_bug","description":"  "}"#.into()])
            .is_err()
    );
}

fn representative_output(command: CliCommand) -> ProviderCommandOutput {
    match command {
        CliCommand::ShowMr { .. }
        | CliCommand::ShowMrForBranch { .. }
        | CliCommand::WaitMr { .. }
        | CliCommand::OpenMr { .. }
        | CliCommand::SetMr { .. }
        | CliCommand::EditMr { .. } => change_request_output(),
        CliCommand::ShowRun { .. } | CliCommand::WaitRun { .. } => ci_run_output(),
        CliCommand::ShowJob { .. } | CliCommand::WaitJob { .. } => ci_job_output(),
        CliCommand::ListThreads { .. } => review_threads_output(),
        CliCommand::ListCi { .. } => ci_runs_and_jobs_output(),
        CliCommand::ListJobs { .. } => ci_jobs_output(),
        CliCommand::LogJob { .. } => ci_job_log_output(),
        CliCommand::CommentMr { .. }
        | CliCommand::ReplyThread { .. }
        | CliCommand::SetThread { .. }
        | CliCommand::RetryJob { .. }
        | CliCommand::CancelJob { .. }
        | CliCommand::CancelRun { .. }
        | CliCommand::ReportWtToolBug { .. }
        | CliCommand::ReportWtToolIssue { .. }
        | CliCommand::SuggestWtToolImprovement { .. }
        | CliCommand::RequestWtToolFeature { .. } => confirmation_output(),
    }
}

fn change_request_output() -> ProviderCommandOutput {
    ProviderCommandOutput::ChangeRequest(ChangeRequestStatus {
        handle: "7".to_owned(),
        url: "https://github.test/acme/widget/pull/7".to_owned(),
        title: "Fix login".to_owned(),
        body: Some("First paragraph.\n\nSecond paragraph.".to_owned()),
        state: "open".to_owned(),
        draft: false,
        head: "abc123".to_owned(),
        base: "main".to_owned(),
        review_state: Some("approved".to_owned()),
        threads: match review_threads_output() {
            ProviderCommandOutput::ReviewThreads(threads) => threads,
            _ => unreachable!(),
        },
        jobs: vec![ci_job()],
    })
}

fn review_threads_output() -> ProviderCommandOutput {
    ProviderCommandOutput::ReviewThreads(vec![ReviewThread {
        handle: ReviewThreadHandle::new("T:thread-1"),
        resolvable: true,
        resolved: false,
        path: Some("src/login.rs".to_owned()),
        line: Some(42),
        comments: vec![ReviewComment {
            author: "reviewer".to_owned(),
            body: "Handle this error.".to_owned(),
            url: Some("https://github.test/thread-1".to_owned()),
        }],
    }])
}

fn ci_run() -> CiRun {
    CiRun {
        handle: "91".to_owned(),
        name: "CI".to_owned(),
        state: "success".to_owned(),
        trigger: Some("pull_request".to_owned()),
        url: Some("https://github.test/runs/91".to_owned()),
        head: "abc123".to_owned(),
        branch: Some("wt/fix".to_owned()),
    }
}

fn ci_run_output() -> ProviderCommandOutput {
    ProviderCommandOutput::CiRun(ci_run())
}

fn ci_job() -> CiJob {
    CiJob {
        handle: CiJobHandle::new("44"),
        run: Some("91".to_owned()),
        name: "checks".to_owned(),
        state: "success".to_owned(),
        url: Some("https://github.test/jobs/44".to_owned()),
    }
}

fn ci_job_output() -> ProviderCommandOutput {
    ProviderCommandOutput::CiJob(ci_job())
}

fn ci_jobs_output() -> ProviderCommandOutput {
    ProviderCommandOutput::CiJobs(vec![ci_job()])
}

fn ci_runs_and_jobs_output() -> ProviderCommandOutput {
    ProviderCommandOutput::CiRunsAndJobs {
        runs: vec![ci_run()],
        jobs: vec![ci_job()],
    }
}

fn ci_job_log_output() -> ProviderCommandOutput {
    ProviderCommandOutput::CiJobLog("build complete\n".to_owned())
}

fn confirmation_output() -> ProviderCommandOutput {
    ProviderCommandOutput::Confirmation("Operation completed.".to_owned())
}
