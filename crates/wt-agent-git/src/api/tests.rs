use super::*;

#[test]
fn ci_output_includes_the_trigger_event() {
    let run = CiRun {
        handle: "91".to_owned(),
        name: "CI".to_owned(),
        state: "success".to_owned(),
        trigger: Some("pull_request".to_owned()),
        url: Some("https://github.test/runs/91".to_owned()),
        head: "abc123".to_owned(),
        branch: Some("wt/fix".to_owned()),
    };

    insta::assert_snapshot!(
        render_cli_command_output(ProviderCommandOutput::CiRun(run.clone())),
        @r###"
    Run: 91
    State: success
    Name: CI
    Trigger: pull_request
    Commit: abc123
    Ref: wt/fix
    URL: https://github.test/runs/91
    "###
    );
    insta::assert_snapshot!(
        render_cli_command_output(ProviderCommandOutput::CiRunsAndJobs {
            runs: vec![run],
            jobs: Vec::new(),
        }),
        @r###"
    run 91 [success] CI trigger=pull_request
    "###
    );
}

#[test]
fn merge_request_output_includes_the_body() {
    let request = ChangeRequestStatus {
        handle: "#7".to_owned(),
        url: "https://github.test/pull/7".to_owned(),
        title: "Fix login".to_owned(),
        body: Some("First paragraph.\n\nSecond paragraph.".to_owned()),
        state: "open".to_owned(),
        draft: false,
        head: "abc123".to_owned(),
        base: "main".to_owned(),
        review_state: None,
        threads: Vec::new(),
        jobs: Vec::new(),
    };

    insta::assert_snapshot!(
        render_cli_command_output(ProviderCommandOutput::ChangeRequest(request)),
        @r###"
    MR: #7
    State: open
    Title: Fix login
    Head: abc123
    Base: main
    URL: https://github.test/pull/7
    Body:
    First paragraph.

    Second paragraph.
    "###
    );
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
    for json in [
        r#"{"action":"show_mr","mr":7}"#,
        r#"{"action":"show_mr_for_branch","branch":"wt/fix"}"#,
        r#"{"action":"show_run","run":91}"#,
        r#"{"action":"show_job","job":44}"#,
        r#"{"action":"list_threads","mr":7}"#,
        r#"{"action":"list_ci","commit":"abc1234"}"#,
        r#"{"action":"list_jobs","run":91}"#,
        r#"{"action":"log_job","job":44}"#,
        r#"{"action":"wait_mr","mr":7}"#,
        r#"{"action":"wait_run","run":91}"#,
        r#"{"action":"wait_job","job":44}"#,
        r#"{"action":"open_mr","head":"wt/fix","base":"main"}"#,
        r#"{"action":"set_mr","mr":7,"state":"ready"}"#,
        r#"{"action":"edit_mr","mr":7,"title":"Fix login"}"#,
        r#"{"action":"comment_mr","mr":7,"body":"Done"}"#,
        r#"{"action":"reply_thread","mr":7,"thread":"T1","body":"Done"}"#,
        r#"{"action":"set_thread","mr":7,"thread":"T1","resolved":true}"#,
        r#"{"action":"retry_job","job":44}"#,
        r#"{"action":"cancel_job","job":44}"#,
        r#"{"action":"cancel_run","run":91}"#,
        r#"{"action":"report_ag_git_bug","description":"build failed"}"#,
        r#"{"action":"report_ag_git_issue","description":"output is unclear"}"#,
        r#"{"action":"suggest_ag_git_improvement","description":"show progress"}"#,
        r#"{"action":"request_ag_git_feature","description":"add search"}"#,
    ] {
        CliCommand::parse(&[json.to_owned()]).unwrap();
    }
    assert_eq!(
        CliCommand::parse(&[r#"{"action":"wait_job","job":42}"#.into()]).unwrap(),
        CliCommand::WaitJob { job: 42 }
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
        CliCommand::parse(&[r#"{"action":"report_ag_git_bug","description":"  "}"#.into()])
            .is_err()
    );
}

#[test]
fn review_output_includes_actionable_commands() {
    let threads = vec![ReviewThread {
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
    }];

    insta::assert_snapshot!(render_threads(&threads), @r###"
    T:thread-1 [open] src/login.rs:42
      reviewer: Handle this error.
      https://github.test/thread-1
    "###);
}
