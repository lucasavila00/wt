use super::*;

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
fn command_parser_rejects_ambiguous_inputs() {
    assert_eq!(
        CliCommand::parse(&["wait".into(), "job".into(), "42".into()]).unwrap(),
        CliCommand::WaitJob { job: 42 }
    );
    assert_eq!(
        CliCommand::parse(&[
            "open".into(),
            "mr".into(),
            "--head".into(),
            "wt/fix".into(),
            "--base".into(),
            "main".into(),
            "--draft".into(),
        ])
        .unwrap(),
        CliCommand::OpenMr {
            head: "wt/fix".to_owned(),
            base: "main".to_owned(),
            draft: true,
        }
    );
    assert!(CliCommand::parse(&[]).is_err());
    assert!(CliCommand::parse(&["wait".into()]).is_err());
    assert!(CliCommand::parse(&["log".into(), "42".into()]).is_err());
    assert!(CliCommand::parse(&["open-mr".into()]).is_err());
}

#[test]
fn command_errors_include_complete_agent_context() {
    let scope = ProviderCommandScope {
        host: "github.example",
        project: "acme/widget",
        base: "main",
        prefix: "df1/",
        branch: "df1/fix-login",
        head: "abc123",
    };
    let error = with_provider_command_context(
        Err(anyhow::anyhow!(
            "review thread `T9` was not found; run `ag-git list threads mr ID` and use its provider ID"
        )),
        ProviderKind::GitHub,
        &scope,
        &ProviderCommand::SetReviewThreadResolved {
            thread: ReviewThreadHandle::new("T9"),
            resolved: true,
        },
    )
    .unwrap_err();

    insta::assert_snapshot!(format!("{error:#}"), @r###"
    ag-git could not resolve the review thread
    Provider: GitHub (github.example)
    Project: acme/widget
    Branch: df1/fix-login
    Base: main
    Current commit: abc123
    Cause: review thread `T9` was not found; run `ag-git list threads mr ID` and use its provider ID
    "###);
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
