use super::*;
use crate::api::render_cli_command_output;
use crate::api::test_server::{serve, ExpectedRequest};

const MR: &str = r#"{"iid":8,"title":"Fix login","description":"Fixes the login flow.","web_url":"https://gitlab.test/acme/widget/-/merge_requests/8","state":"merged","draft":false,"sha":"abc123","source_branch":"wt/fix-login","target_branch":"main","source_project_id":12,"target_project_id":12,"has_conflicts":true,"detailed_merge_status":"conflict"}"#;
const OPEN_MR: &str = r#"{"iid":8,"title":"Fix login","description":"Fixes the login flow.","web_url":"https://gitlab.test/acme/widget/-/merge_requests/8","state":"opened","draft":false,"sha":"abc123","source_branch":"wt/fix-login","target_branch":"main","source_project_id":12,"target_project_id":12,"has_conflicts":false,"detailed_merge_status":"checking"}"#;
const PIPELINE: &str = r#"{"id":92,"status":"success","web_url":"https://gitlab.test/pipelines/92","sha":"abc123","ref":"wt/fix-login","source":"merge_request_event"}"#;
const JOB: &str = r#"{"id":45,"name":"test","status":"success","web_url":"https://gitlab.test/jobs/45","ref":"wt/fix-login","pipeline":{"id":92}}"#;
const THREADS: &str = r#"{"data":{"project":{"mergeRequest":{"id":"gid://gitlab/MergeRequest/8","diffHeadSha":"abc123","discussions":{"pageInfo":{"hasNextPage":false},"nodes":[{"id":"gid://gitlab/Discussion/thread-8","resolved":false,"resolvable":true,"notes":{"pageInfo":{"hasNextPage":false},"nodes":[{"author":{"username":"reviewer"},"body":"Please clarify this.","url":"https://gitlab.test/thread/8","position":{"filePath":"src/lib.rs","newLine":12,"oldLine":null}}]}}]}}}}}"#;
const NOTE: &str = r#"{"id":124,"body":"General feedback.","author":{"username":"reviewer"},"created_at":"2026-08-22T10:00:00Z","updated_at":"2026-08-22T11:00:00Z","system":false,"resolvable":false}"#;
const GATEWAY_NOTE: &str = r#"{"id":124,"body":"General feedback.\n\n— WT world `wt`","author":{"username":"agent"},"created_at":"2026-08-22T10:00:00Z","updated_at":"2026-08-22T11:00:00Z","system":false,"resolvable":false}"#;
const CREATED_NOTE: &str = r#"{"id":125,"body":"Ready for another look.\n\n— WT world `wt`","author":{"username":"agent"},"created_at":"2026-08-23T10:00:00Z","updated_at":"2026-08-23T10:00:00Z","system":false,"resolvable":false}"#;
const UPDATED_NOTE: &str = r#"{"id":124,"body":"Updated.\n\n— WT world `wt`","author":{"username":"agent"},"created_at":"2026-08-22T10:00:00Z","updated_at":"2026-08-23T10:00:00Z","system":false,"resolvable":false}"#;

#[test]
fn cli_commands_render_provider_results_as_json() {
    let cases = [
        ("show_mr", WtToolsCommand::ShowMr { mr: "8".into() }),
        (
            "show_mr_for_branch",
            WtToolsCommand::ShowMrForBranch {
                branch: "wt/fix-login".to_owned(),
            },
        ),
        ("show_run", WtToolsCommand::ShowRun { run: "92".into() }),
        ("show_job", WtToolsCommand::ShowJob { job: "45".into() }),
        (
            "list_threads",
            WtToolsCommand::ListThreads { mr: "8".into() },
        ),
        (
            "list_comments",
            WtToolsCommand::ListComments { mr: "8".into() },
        ),
        (
            "show_comment",
            WtToolsCommand::ShowComment {
                mr: "8".into(),
                comment: "124".into(),
            },
        ),
        (
            "list_ci",
            WtToolsCommand::ListCi {
                commit: "abc123".to_owned(),
            },
        ),
        ("list_jobs", WtToolsCommand::ListJobs { run: "92".into() }),
        ("log_job", WtToolsCommand::LogJob { job: "45".into() }),
        (
            "wait_mr",
            WtToolsCommand::WaitMr {
                mr: "8".into(),
                timeout_seconds: None,
            },
        ),
        (
            "wait_run",
            WtToolsCommand::WaitRun {
                run: "92".into(),
                timeout_seconds: None,
            },
        ),
        (
            "wait_job",
            WtToolsCommand::WaitJob {
                job: "45".into(),
                timeout_seconds: None,
            },
        ),
        ("retry_job", WtToolsCommand::RetryJob { job: "45".into() }),
        ("cancel_job", WtToolsCommand::CancelJob { job: "45".into() }),
        ("cancel_run", WtToolsCommand::CancelRun { run: "92".into() }),
        (
            "open_mr",
            WtToolsCommand::OpenMr {
                head: "wt/fix-login".to_owned(),
                base: "main".to_owned(),
            },
        ),
        (
            "set_mr",
            WtToolsCommand::SetMr {
                mr: "8".into(),
                state: ChangeRequestState::Closed,
                confirm_merged: false,
            },
        ),
        (
            "edit_mr",
            WtToolsCommand::EditMr {
                mr: "8".into(),
                title: Some("Clarify login fix".to_owned()),
                body: None,
                confirm_merged: false,
            },
        ),
        (
            "comment_mr",
            WtToolsCommand::CommentMr {
                mr: "8".into(),
                body: "Ready for another look.".to_owned(),
                confirm_merged: false,
            },
        ),
        (
            "edit_comment",
            WtToolsCommand::EditComment {
                mr: "8".into(),
                comment: "124".into(),
                body: "Updated.".to_owned(),
                confirm_merged: false,
            },
        ),
        (
            "delete_comment",
            WtToolsCommand::DeleteComment {
                mr: "8".into(),
                comment: "124".into(),
                confirm_merged: false,
            },
        ),
        (
            "reply_thread",
            WtToolsCommand::ReplyThread {
                mr: "8".into(),
                thread: "gid://gitlab/Discussion/thread-8".to_owned(),
                body: "Fixed.".to_owned(),
                confirm_merged: false,
            },
        ),
        (
            "set_thread",
            WtToolsCommand::SetThread {
                mr: "8".into(),
                thread: "gid://gitlab/Discussion/thread-8".to_owned(),
                resolved: true,
                confirm_merged: false,
            },
        ),
    ];

    for (name, command) in cases {
        let (base_url, server) = serve(fixtures(&command));
        let provider = GitlabApi::with_base_url(base_url, "fixture-token").unwrap();
        let output = provider
            .execute_cli_command(&project_scope(), &command)
            .unwrap();

        let rendered = render_cli_command_output(output);
        let message: serde_json::Value = serde_json::from_str(&rendered).unwrap();
        insta::assert_snapshot!(name, serde_json::to_string_pretty(&message).unwrap());
        server.join().unwrap().unwrap();
    }
}

#[test]
fn merged_merge_requests_require_confirmation_before_modification() {
    let commands = [
        WtToolsCommand::SetMr {
            mr: "8".into(),
            state: ChangeRequestState::Closed,
            confirm_merged: false,
        },
        WtToolsCommand::EditMr {
            mr: "8".into(),
            title: Some("Better title".to_owned()),
            body: None,
            confirm_merged: false,
        },
        WtToolsCommand::CommentMr {
            mr: "8".into(),
            body: "Done".to_owned(),
            confirm_merged: false,
        },
        WtToolsCommand::EditComment {
            mr: "8".into(),
            comment: "124".into(),
            body: "Done".to_owned(),
            confirm_merged: false,
        },
        WtToolsCommand::DeleteComment {
            mr: "8".into(),
            comment: "124".into(),
            confirm_merged: false,
        },
        WtToolsCommand::ReplyThread {
            mr: "8".into(),
            thread: "gid://gitlab/Discussion/thread-8".to_owned(),
            body: "Done".to_owned(),
            confirm_merged: false,
        },
        WtToolsCommand::SetThread {
            mr: "8".into(),
            thread: "gid://gitlab/Discussion/thread-8".to_owned(),
            resolved: true,
            confirm_merged: false,
        },
    ];

    for command in commands {
        let (base_url, server) = serve(vec![get(
            "/api/v4/projects/acme%2Fwidget/merge_requests/8",
            MR,
        )]);
        let provider = GitlabApi::with_base_url(base_url, "fixture-token").unwrap();

        let error = provider
            .execute_cli_command(&project_scope(), &command)
            .unwrap_err();

        assert_eq!(
            error.to_string(),
            "MR 8 is already merged; rerun with `confirm_merged`: true to modify it"
        );
        server.join().unwrap().unwrap();
    }
}

#[test]
fn wait_timeouts_preserve_the_last_observed_state() {
    let cases = [
        (
            WtToolsCommand::WaitMr {
                mr: "8".into(),
                timeout_seconds: Some(0),
            },
            get("/api/v4/projects/acme%2Fwidget/merge_requests/8", OPEN_MR),
            "mr 8",
            "opened",
        ),
        (
            WtToolsCommand::WaitJob {
                job: "45".into(),
                timeout_seconds: Some(0),
            },
            get(
                "/api/v4/projects/acme%2Fwidget/jobs/45",
                JOB.replace(r#""status":"success""#, r#""status":"running""#)
                    .leak(),
            ),
            "job 45",
            "running",
        ),
    ];

    for (command, request, resource, last_state) in cases {
        let (base_url, server) = serve(vec![request]);
        let provider = GitlabApi::with_base_url(base_url, "fixture-token").unwrap();

        assert_eq!(
            provider
                .execute_cli_command(&project_scope(), &command)
                .unwrap(),
            ProviderCommandOutput::WaitTimeout {
                resource: resource.to_owned(),
                last_state: last_state.to_owned(),
            }
        );
        server.join().unwrap().unwrap();
    }
}

#[test]
fn wait_run_timeout_returns_the_unfinished_run() {
    let response = PIPELINE
        .replace(r#""status":"success""#, r#""status":"running""#)
        .leak();
    let (base_url, server) = serve(vec![get(
        "/api/v4/projects/acme%2Fwidget/pipelines/92",
        response,
    )]);
    let provider = GitlabApi::with_base_url(base_url, "fixture-token").unwrap();

    let output = provider
        .execute_cli_command(
            &project_scope(),
            &WtToolsCommand::WaitRun {
                run: "92".into(),
                timeout_seconds: Some(0),
            },
        )
        .unwrap();

    insta::assert_snapshot!(render_cli_command_output(output), @r###"
    {"type":"ci_run","data":{"handle":"92","name":"pipeline","state":"running","trigger":"merge_request_event","url":"https://gitlab.test/pipelines/92","head":"abc123","branch":"wt/fix-login"}}
    "###);
    server.join().unwrap().unwrap();
}

#[test]
fn list_comments_reads_every_rest_page() {
    let first_page = format!("[{}]", vec![NOTE; 100].join(",")).leak();
    let (base_url, server) = serve(vec![
        get("/api/v4/projects/acme%2Fwidget/merge_requests/8", MR),
        get(
            "/api/v4/projects/acme%2Fwidget/merge_requests/8/notes?per_page=100&page=1&sort=asc&order_by=created_at",
            first_page,
        ),
        get(
            "/api/v4/projects/acme%2Fwidget/merge_requests/8/notes?per_page=100&page=2&sort=asc&order_by=created_at",
            "[]",
        ),
    ]);
    let provider = GitlabApi::with_base_url(base_url, "fixture-token").unwrap();

    let ProviderCommandOutput::GeneralComments(comments) = provider
        .execute_cli_command(
            &project_scope(),
            &WtToolsCommand::ListComments { mr: "8".into() },
        )
        .unwrap()
    else {
        panic!("expected general comments");
    };

    assert_eq!(comments.len(), 100);
    server.join().unwrap().unwrap();
}

#[test]
fn show_comment_rejects_a_resolvable_review_note() {
    let review_note = NOTE
        .replace("\"resolvable\":false", "\"resolvable\":true")
        .leak();
    let (base_url, server) = serve(vec![
        get("/api/v4/projects/acme%2Fwidget/merge_requests/8", MR),
        get(
            "/api/v4/projects/acme%2Fwidget/merge_requests/8/notes/124",
            review_note,
        ),
    ]);
    let provider = GitlabApi::with_base_url(base_url, "fixture-token").unwrap();

    let error = provider
        .execute_cli_command(
            &project_scope(),
            &WtToolsCommand::ShowComment {
                mr: "8".into(),
                comment: "124".into(),
            },
        )
        .unwrap_err();

    assert_eq!(
        error.to_string(),
        "comment 124 is not a general comment in MR 8"
    );
    server.join().unwrap().unwrap();
}

#[test]
fn comment_mutations_reject_a_resolvable_review_note_before_writing() {
    let review_note = NOTE
        .replace("\"resolvable\":false", "\"resolvable\":true")
        .leak();
    let commands = [
        WtToolsCommand::EditComment {
            mr: "8".into(),
            comment: "124".into(),
            body: "Updated.".to_owned(),
            confirm_merged: false,
        },
        WtToolsCommand::DeleteComment {
            mr: "8".into(),
            comment: "124".into(),
            confirm_merged: false,
        },
    ];

    for command in commands {
        let (base_url, server) = serve(vec![
            get("/api/v4/projects/acme%2Fwidget/merge_requests/8", OPEN_MR),
            get("/api/v4/projects/acme%2Fwidget/merge_requests/8", OPEN_MR),
            get(
                "/api/v4/projects/acme%2Fwidget/merge_requests/8/notes/124",
                review_note,
            ),
        ]);
        let provider = GitlabApi::with_base_url(base_url, "fixture-token").unwrap();

        let error = provider
            .execute_cli_command(&project_scope(), &command)
            .unwrap_err();

        assert_eq!(
            error.to_string(),
            "comment 124 is not a general comment in MR 8"
        );
        server.join().unwrap().unwrap();
    }
}

fn fixtures(command: &WtToolsCommand) -> Vec<ExpectedRequest> {
    match command {
        WtToolsCommand::ShowMr { .. } => vec![
            get("/api/v4/projects/acme%2Fwidget/merge_requests/8", MR),
            get(
                "/api/v4/projects/acme%2Fwidget/pipelines?sha=abc123&per_page=100",
                "[]",
            ),
        ],
        WtToolsCommand::WaitMr { .. } => vec![get(
            "/api/v4/projects/acme%2Fwidget/merge_requests/8",
            MR,
        )],
        WtToolsCommand::ShowMrForBranch { .. } => vec![
            get(
                "/api/v4/projects/acme%2Fwidget/merge_requests?state=opened&source_branch=wt%2Ffix-login&per_page=100",
                format!("[{OPEN_MR}]").leak(),
            ),
            get(
                "/api/v4/projects/acme%2Fwidget/pipelines?sha=abc123&per_page=100",
                "[]",
            ),
        ],
        WtToolsCommand::ShowRun { .. } | WtToolsCommand::WaitRun { .. } => vec![get(
            "/api/v4/projects/acme%2Fwidget/pipelines/92",
            PIPELINE,
        )],
        WtToolsCommand::ShowJob { .. } | WtToolsCommand::WaitJob { .. } => vec![get(
            "/api/v4/projects/acme%2Fwidget/jobs/45",
            JOB,
        )],
        WtToolsCommand::ListThreads { .. } => vec![graphql("GitlabReadMergeRequestByIid", THREADS)],
        WtToolsCommand::ListComments { .. } => vec![
            get("/api/v4/projects/acme%2Fwidget/merge_requests/8", MR),
            get(
                "/api/v4/projects/acme%2Fwidget/merge_requests/8/notes?per_page=100&page=1&sort=asc&order_by=created_at",
                format!(
                    "[{NOTE},{},{}]",
                    NOTE.replace("\"id\":124", "\"id\":125")
                        .replace("\"system\":false", "\"system\":true"),
                    NOTE.replace("\"id\":124", "\"id\":126")
                        .replace("\"resolvable\":false", "\"resolvable\":true")
                )
                .leak(),
            ),
        ],
        WtToolsCommand::ShowComment { .. } => vec![
            get("/api/v4/projects/acme%2Fwidget/merge_requests/8", MR),
            get(
                "/api/v4/projects/acme%2Fwidget/merge_requests/8/notes/124",
                NOTE,
            ),
        ],
        WtToolsCommand::ListCi { .. } => vec![
            get(
                "/api/v4/projects/acme%2Fwidget/pipelines?sha=abc123&per_page=100",
                format!("[{PIPELINE}]").leak(),
            ),
            get(
                "/api/v4/projects/acme%2Fwidget/pipelines/92/jobs?include_retried=false&per_page=100",
                format!("[{JOB}]").leak(),
            ),
        ],
        WtToolsCommand::ListJobs { .. } => vec![get(
            "/api/v4/projects/acme%2Fwidget/pipelines/92/jobs?include_retried=false&per_page=100",
            format!("[{JOB}]").leak(),
        )],
        WtToolsCommand::LogJob { .. } => vec![ExpectedRequest {
            method: "GET",
            path: "/api/v4/projects/acme%2Fwidget/jobs/45/trace",
            required_header: Some(("private-token", "fixture-token")),
            body_contains: None,
            response_content_type: "text/plain",
            response_body: "build complete\n",
        }],
        WtToolsCommand::RetryJob { .. } => job_action("retry"),
        WtToolsCommand::CancelJob { .. } => job_action("cancel"),
        WtToolsCommand::CancelRun { .. } => vec![
            get("/api/v4/projects/acme%2Fwidget/pipelines/92", PIPELINE),
            post("/api/v4/projects/acme%2Fwidget/pipelines/92/cancel", "{}"),
        ],
        WtToolsCommand::OpenMr { .. } => vec![
            get(
                "/api/v4/projects/acme%2Fwidget/repository/commits/wt%2Ffix-login",
                r#"{"id":"abc123"}"#,
            ),
            graphql("GitlabReadMergeRequest", NO_MERGE_REQUEST_RESPONSE),
            graphql(
                "GitlabCreateMergeRequest",
                r#"{"data":{"mergeRequestCreate":{"errors":[],"mergeRequest":{"id":"merge-request-8","iid":"8","webUrl":"https://gitlab.test/acme/widget/-/merge_requests/8"}}}}"#,
            ),
            graphql("GitlabReadMergeRequest", MERGE_REQUEST_RESPONSE),
            graphql(
                "GitlabSetMergeRequestDraft",
                r#"{"data":{"mergeRequestSetDraft":{"errors":[],"mergeRequest":{"id":"merge-request-8","iid":"8","webUrl":"https://gitlab.test/acme/widget/-/merge_requests/8"}}}}"#,
            ),
            graphql(
                "GitlabReadMergeRequest",
                MERGE_REQUEST_RESPONSE
                    .replace("\"draft\": false", "\"draft\": true")
                    .leak(),
            ),
        ],
        WtToolsCommand::SetMr { .. } | WtToolsCommand::EditMr { .. } => vec![
            get("/api/v4/projects/acme%2Fwidget/merge_requests/8", OPEN_MR),
            graphql(
                "GitlabUpdateMergeRequest",
                r#"{"data":{"mergeRequestUpdate":{"errors":[],"mergeRequest":{"id":"merge-request-8","iid":"8","webUrl":"https://gitlab.test/acme/widget/-/merge_requests/8"}}}}"#,
            ),
            get("/api/v4/projects/acme%2Fwidget/merge_requests/8", OPEN_MR),
        ],
        WtToolsCommand::CommentMr { .. } => vec![
            get("/api/v4/projects/acme%2Fwidget/merge_requests/8", OPEN_MR),
            write(
                "POST",
                "/api/v4/projects/acme%2Fwidget/merge_requests/8/notes",
                CREATED_NOTE,
            ),
        ],
        WtToolsCommand::EditComment { .. } => vec![
            get("/api/v4/projects/acme%2Fwidget/merge_requests/8", OPEN_MR),
            get("/api/v4/projects/acme%2Fwidget/merge_requests/8", OPEN_MR),
            get(
                "/api/v4/projects/acme%2Fwidget/merge_requests/8/notes/124",
                GATEWAY_NOTE,
            ),
            write(
                "PUT",
                "/api/v4/projects/acme%2Fwidget/merge_requests/8/notes/124",
                UPDATED_NOTE,
            ),
        ],
        WtToolsCommand::DeleteComment { .. } => vec![
            get("/api/v4/projects/acme%2Fwidget/merge_requests/8", OPEN_MR),
            get("/api/v4/projects/acme%2Fwidget/merge_requests/8", OPEN_MR),
            get(
                "/api/v4/projects/acme%2Fwidget/merge_requests/8/notes/124",
                GATEWAY_NOTE,
            ),
            request(
                "DELETE",
                "/api/v4/projects/acme%2Fwidget/merge_requests/8/notes/124",
                None,
                "application/json",
                "{}",
            ),
        ],
        WtToolsCommand::ReplyThread { .. } => vec![
            get("/api/v4/projects/acme%2Fwidget/merge_requests/8", OPEN_MR),
            graphql("GitlabReadMergeRequestByIid", THREADS),
            graphql(
                "GitlabReplyToDiscussion",
                r#"{"data":{"createNote":{"errors":[],"note":{"id":"note-2","url":null}}}}"#,
            ),
        ],
        WtToolsCommand::SetThread { .. } => vec![
            get("/api/v4/projects/acme%2Fwidget/merge_requests/8", OPEN_MR),
            graphql("GitlabReadMergeRequestByIid", THREADS),
            graphql(
                "GitlabSetDiscussionResolved",
                r#"{"data":{"discussionToggleResolve":{"errors":[],"discussion":{"id":"gid://gitlab/Discussion/thread-8","resolved":true}}}}"#,
            ),
        ],
    }
}

fn job_action(action: &'static str) -> Vec<ExpectedRequest> {
    vec![
        get("/api/v4/projects/acme%2Fwidget/jobs/45", JOB),
        post(
            if action == "retry" {
                "/api/v4/projects/acme%2Fwidget/jobs/45/retry"
            } else {
                "/api/v4/projects/acme%2Fwidget/jobs/45/cancel"
            },
            "{}",
        ),
    ]
}

fn get(path: &'static str, response_body: &'static str) -> ExpectedRequest {
    request("GET", path, None, "application/json", response_body)
}

fn post(path: &'static str, response_body: &'static str) -> ExpectedRequest {
    request("POST", path, None, "application/json", response_body)
}

fn write(method: &'static str, path: &'static str, response_body: &'static str) -> ExpectedRequest {
    request(
        method,
        path,
        Some("\"body\":"),
        "application/json",
        response_body,
    )
}

fn graphql(operation: &'static str, response_body: &'static str) -> ExpectedRequest {
    request(
        "POST",
        "/api/graphql",
        Some(operation),
        "application/json",
        response_body,
    )
}

fn request(
    method: &'static str,
    path: &'static str,
    body_contains: Option<&'static str>,
    response_content_type: &'static str,
    response_body: &'static str,
) -> ExpectedRequest {
    ExpectedRequest {
        method,
        path,
        required_header: Some(("private-token", "fixture-token")),
        body_contains,
        response_content_type,
        response_body,
    }
}
