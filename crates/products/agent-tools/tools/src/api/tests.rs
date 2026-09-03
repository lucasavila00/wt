use super::*;

#[test]
fn generated_typescript_is_the_complete_command_contract() {
    insta::assert_snapshot!(TYPESCRIPT_COMMAND_TYPE);
}

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
    {"type":"ci_run","data":{"handle":"91","name":"CI","state":"success","trigger":"pull_request","url":"https://github.test/runs/91","head":"abc123","branch":"wt/fix"}}
    "###
    );
    insta::assert_snapshot!(
        render_cli_command_output(ProviderCommandOutput::CiRunsAndJobs {
            runs: vec![run],
            jobs: Vec::new(),
        }),
        @r###"
    {"type":"ci_runs_and_jobs","data":{"runs":[{"handle":"91","name":"CI","state":"success","trigger":"pull_request","url":"https://github.test/runs/91","head":"abc123","branch":"wt/fix"}],"jobs":[]}}
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
        conflict_state: None,
        review_state: None,
        threads: Vec::new(),
        jobs: Vec::new(),
    };

    insta::assert_snapshot!(
        render_cli_command_output(ProviderCommandOutput::ChangeRequest(request)),
        @r###"
    {"type":"change_request","data":{"handle":"#7","url":"https://github.test/pull/7","title":"Fix login","body":"First paragraph.\n\nSecond paragraph.","state":"open","draft":false,"head":"abc123","base":"main","review_state":null,"threads":[],"jobs":[]}}
    "###
    );
}

#[test]
fn ci_job_logs_keep_only_a_bounded_tail() {
    let output = render_cli_command_output(ProviderCommandOutput::CiJobLog {
        log: "last lines\n".to_owned(),
        truncated: true,
    });

    insta::assert_snapshot!(output, @r###"
    {"type":"ci_job_log","data":{"log":"[earlier CI log output omitted]\nlast lines\n","truncated":true}}
    "###);
}

#[test]
fn command_parser_accepts_only_valid_json_objects() {
    let provider_commands = [
        r#"{"action":"show_mr","mr":"7"}"#,
        r#"{"action":"show_mr_for_branch","branch":"wt/fix"}"#,
        r#"{"action":"show_run","run":"91"}"#,
        r#"{"action":"show_job","job":"44"}"#,
        r#"{"action":"list_threads","mr":"7"}"#,
        r#"{"action":"list_comments","mr":"7"}"#,
        r#"{"action":"show_comment","mr":"7","comment":"123"}"#,
        r#"{"action":"edit_comment","mr":"7","comment":"123","body":"Updated"}"#,
        r#"{"action":"delete_comment","mr":"7","comment":"123"}"#,
        r#"{"action":"list_ci","commit":"abc1234"}"#,
        r#"{"action":"list_jobs","run":"91"}"#,
        r#"{"action":"log_job","job":"44"}"#,
        r#"{"action":"wait_mr","mr":"7"}"#,
        r#"{"action":"wait_run","run":"91"}"#,
        r#"{"action":"wait_job","job":"44"}"#,
        r#"{"action":"open_mr","head":"wt/fix","base":"main"}"#,
        r#"{"action":"set_mr","mr":"7","state":"ready"}"#,
        r#"{"action":"edit_mr","mr":"7","title":"Fix login"}"#,
        r#"{"action":"comment_mr","mr":"7","body":"Done"}"#,
        r#"{"action":"reply_thread","mr":"7","thread":"T1","body":"Done"}"#,
        r#"{"action":"set_thread","mr":"7","thread":"T1","resolved":true}"#,
        r#"{"action":"retry_job","job":"44"}"#,
        r#"{"action":"cancel_job","job":"44"}"#,
        r#"{"action":"cancel_run","run":"91"}"#,
    ];
    for command in provider_commands {
        let json = format!(
            r#"{{"target":{{"provider":"github","repository":"acme/widget"}},"command":{command}}}"#
        );
        WtToolsCommand::parse(&[json]).unwrap();
    }
    for command in [
        r#"{"action":"report_wt_tool_bug","description":"build failed"}"#,
        r#"{"action":"report_wt_tool_issue","description":"output is unclear"}"#,
        r#"{"action":"suggest_wt_tool_improvement","description":"show progress"}"#,
        r#"{"action":"request_wt_tool_feature","description":"add search"}"#,
    ] {
        WtToolsCommand::parse(&[format!(r#"{{"command":{command}}}"#)]).unwrap();
    }
    assert!(WtToolsCommand::parse(&[
        r#"{"command":{"action":"send_message_to_parent","message":"done"}}"#.into(),
    ])
    .is_ok());
    assert!(WtToolsCommand::parse(&[
        r#"{"command":{"action":"send_message_to_parent","message":""}}"#.into(),
    ])
    .is_err());
    let targeted = |command: &str| {
        format!(
            r#"{{"target":{{"provider":"github","repository":"acme/widget"}},"command":{command}}}"#
        )
    };
    let parsed_command = |command: &str| match WtToolsCommand::parse(&[targeted(command)]).unwrap()
    {
        WtToolsCommand::GitHosting { command, .. } => command,
        WtToolsCommand::Feedback { .. } => panic!("expected Git hosting command"),
        WtToolsCommand::World { .. } => panic!("expected Git hosting command"),
    };
    assert_eq!(
        parsed_command(r#"{"action":"wait_job","job":"42"}"#),
        GitHostingCommand::WaitJob {
            job: "42".into(),
            timeout_seconds: None,
        }
    );
    assert!(WtToolsCommand::parse(&[targeted(
        r#"{"action":"wait_run","run":"91","timeout_seconds":0}"#
    )])
    .is_err());
    assert!(WtToolsCommand::parse(&[targeted(
        r#"{"action":"show_comment","mr":"7","comment":""}"#
    )])
    .is_err());
    assert!(WtToolsCommand::parse(&[targeted(
        r#"{"action":"edit_comment","mr":"7","comment":""}"#
    )])
    .is_err());
    assert_eq!(
        parsed_command(r#"{"action":"show_mr_for_branch","branch":"wt/fix"}"#),
        GitHostingCommand::ShowMrForBranch {
            branch: "wt/fix".to_owned(),
        }
    );
    assert!(WtToolsCommand::parse(&[targeted(
        r#"{"action":"open_mr","head":"wt/fix","base":"main","draft":false}"#
    )])
    .is_err());
    assert!(WtToolsCommand::parse(&[]).is_err());
    assert!(WtToolsCommand::parse(&["show".into(), "mr".into(), "7".into()]).is_err());
    assert!(WtToolsCommand::parse(&[targeted(r#"{"action":"show_mr","mr":""}"#)]).is_err());
    assert!(WtToolsCommand::parse(&[targeted(r#"{"action":"show_mr","mr":7}"#)]).is_err());
    assert!(
        WtToolsCommand::parse(&[targeted(r#"{"action":"show_mr","mr":"7","extra":true}"#)])
            .is_err()
    );
    assert!(WtToolsCommand::parse(&[targeted(r#"{"action":"edit_mr","mr":"7"}"#)]).is_err());
    assert!(WtToolsCommand::parse(&[
        r#"{"command":{"action":"report_wt_tool_bug","description":"  "}}"#.into()
    ])
    .is_err());
    assert!(
        WtToolsCommand::parse(&[r#"{"command":{"action":"show_mr","mr":"7"}}"#.into()]).is_err()
    );
    assert!(WtToolsCommand::parse(&[r#"{"target":{"provider":"github","repository":"acme/widget"},"command":{"action":"report_wt_tool_bug","description":"bad"}}"#.into()]).is_err());
}

#[test]
fn provider_resource_ids_are_positive_integer_strings() {
    assert_eq!(cli::parse_resource_id("7", "MR").unwrap(), 7);
    assert!(cli::parse_resource_id("0", "MR").is_err());
    assert!(cli::parse_resource_id("not-an-id", "MR").is_err());
}

#[test]
fn wt_tools_comment_marker_is_a_required_visible_prefix() {
    let scope = ProviderProjectScope {
        host: "github.test",
        project: "acme/widget",
        prefix: "wt/",
    };
    assert_eq!(
        attributed_project_comment(&scope, "Done."),
        "**Comment from a WT world agent**\n\nDone."
    );

    let comment = GeneralComment {
        handle: GeneralCommentHandle::new("123"),
        author: "agent".to_owned(),
        body: "Done.\n\n**Comment from a WT world agent**".to_owned(),
        url: "https://github.test/acme/widget/pull/7#issuecomment-123".to_owned(),
        created_at: "2026-08-23T10:00:00Z".to_owned(),
        updated_at: "2026-08-23T10:00:00Z".to_owned(),
    };
    assert_eq!(
        require_project_comment_attribution(&scope, &comment)
            .unwrap_err()
            .to_string(),
        "comment is missing the WT world agent marker"
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

    insta::assert_snapshot!(
        serde_json::to_string_pretty(&ProviderCommandOutput::ReviewThreads(threads)).unwrap(),
        @r###"
    {
      "type": "review_threads",
      "data": [
        {
          "handle": "T:thread-1",
          "resolvable": true,
          "resolved": false,
          "path": "src/login.rs",
          "line": 42,
          "comments": [
            {
              "author": "reviewer",
              "body": "Handle this error.",
              "url": "https://github.test/thread-1"
            }
          ]
        }
      ]
    }
    "###
    );
}
