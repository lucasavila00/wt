use super::*;
use crate::api::render_cli_command_output;
use crate::api::test_server::{serve, ExpectedRequest};

const MR: &str = r#"{"iid":8,"title":"Fix login","description":"Fixes the login flow.","web_url":"https://gitlab.test/acme/widget/-/merge_requests/8","state":"merged","draft":false,"sha":"abc123","source_branch":"wt/fix-login","target_branch":"main","source_project_id":12,"target_project_id":12,"has_conflicts":true,"detailed_merge_status":"conflict"}"#;
const OPEN_MR: &str = r#"{"iid":8,"title":"Fix login","description":"Fixes the login flow.","web_url":"https://gitlab.test/acme/widget/-/merge_requests/8","state":"opened","draft":false,"sha":"abc123","source_branch":"wt/fix-login","target_branch":"main","source_project_id":12,"target_project_id":12,"has_conflicts":false,"detailed_merge_status":"checking"}"#;
const PIPELINE: &str = r#"{"id":92,"status":"success","web_url":"https://gitlab.test/pipelines/92","sha":"abc123","ref":"wt/fix-login","source":"merge_request_event"}"#;
const JOB: &str = r#"{"id":45,"name":"test","status":"success","web_url":"https://gitlab.test/jobs/45","ref":"wt/fix-login","pipeline":{"id":92}}"#;
const THREADS: &str = r#"{"data":{"project":{"mergeRequest":{"id":"gid://gitlab/MergeRequest/8","diffHeadSha":"abc123","discussions":{"pageInfo":{"hasNextPage":false},"nodes":[{"id":"gid://gitlab/Discussion/thread-8","resolved":false,"resolvable":true,"notes":{"pageInfo":{"hasNextPage":false},"nodes":[{"author":{"username":"reviewer"},"body":"Please clarify this.","url":"https://gitlab.test/thread/8","position":{"filePath":"src/lib.rs","newLine":12,"oldLine":null}}]}}]}}}}}"#;

#[test]
fn cli_commands_render_provider_results_as_json() {
    let cases = [
        ("show_mr", CliCommand::ShowMr { mr: 8 }),
        (
            "show_mr_for_branch",
            CliCommand::ShowMrForBranch {
                branch: "wt/fix-login".to_owned(),
            },
        ),
        ("show_run", CliCommand::ShowRun { run: 92 }),
        ("show_job", CliCommand::ShowJob { job: 45 }),
        ("list_threads", CliCommand::ListThreads { mr: 8 }),
        (
            "list_ci",
            CliCommand::ListCi {
                commit: "abc123".to_owned(),
            },
        ),
        ("list_jobs", CliCommand::ListJobs { run: 92 }),
        ("log_job", CliCommand::LogJob { job: 45 }),
        (
            "wait_mr",
            CliCommand::WaitMr {
                mr: 8,
                timeout_seconds: None,
            },
        ),
        (
            "wait_run",
            CliCommand::WaitRun {
                run: 92,
                timeout_seconds: None,
            },
        ),
        (
            "wait_job",
            CliCommand::WaitJob {
                job: 45,
                timeout_seconds: None,
            },
        ),
        ("retry_job", CliCommand::RetryJob { job: 45 }),
        ("cancel_job", CliCommand::CancelJob { job: 45 }),
        ("cancel_run", CliCommand::CancelRun { run: 92 }),
        (
            "open_mr",
            CliCommand::OpenMr {
                head: "wt/fix-login".to_owned(),
                base: "main".to_owned(),
                draft: false,
            },
        ),
        (
            "set_mr",
            CliCommand::SetMr {
                mr: 8,
                state: ChangeRequestState::Closed,
            },
        ),
        (
            "edit_mr",
            CliCommand::EditMr {
                mr: 8,
                title: Some("Clarify login fix".to_owned()),
                body: None,
            },
        ),
        (
            "comment_mr",
            CliCommand::CommentMr {
                mr: 8,
                body: "Ready for another look.".to_owned(),
            },
        ),
        (
            "reply_thread",
            CliCommand::ReplyThread {
                mr: 8,
                thread: ReviewThreadHandle::new("gid://gitlab/Discussion/thread-8"),
                body: "Fixed.".to_owned(),
            },
        ),
        (
            "set_thread",
            CliCommand::SetThread {
                mr: 8,
                thread: ReviewThreadHandle::new("gid://gitlab/Discussion/thread-8"),
                resolved: true,
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

fn fixtures(command: &CliCommand) -> Vec<ExpectedRequest> {
    match command {
        CliCommand::ShowMr { .. } | CliCommand::WaitMr { .. } => vec![get(
            "/api/v4/projects/acme%2Fwidget/merge_requests/8",
            MR,
        )],
        CliCommand::ShowMrForBranch { .. } => vec![get(
            "/api/v4/projects/acme%2Fwidget/merge_requests?state=opened&source_branch=wt%2Ffix-login&per_page=100",
            format!("[{OPEN_MR}]").leak(),
        )],
        CliCommand::ShowRun { .. } | CliCommand::WaitRun { .. } => vec![get(
            "/api/v4/projects/acme%2Fwidget/pipelines/92",
            PIPELINE,
        )],
        CliCommand::ShowJob { .. } | CliCommand::WaitJob { .. } => vec![get(
            "/api/v4/projects/acme%2Fwidget/jobs/45",
            JOB,
        )],
        CliCommand::ListThreads { .. } => vec![graphql("GitlabReadMergeRequestByIid", THREADS)],
        CliCommand::ListCi { .. } => vec![
            get(
                "/api/v4/projects/acme%2Fwidget/pipelines?sha=abc123&per_page=100",
                format!("[{PIPELINE}]").leak(),
            ),
            get(
                "/api/v4/projects/acme%2Fwidget/pipelines/92/jobs?include_retried=false&per_page=100",
                format!("[{JOB}]").leak(),
            ),
        ],
        CliCommand::ListJobs { .. } => vec![get(
            "/api/v4/projects/acme%2Fwidget/pipelines/92/jobs?include_retried=false&per_page=100",
            format!("[{JOB}]").leak(),
        )],
        CliCommand::LogJob { .. } => vec![ExpectedRequest {
            method: "GET",
            path: "/api/v4/projects/acme%2Fwidget/jobs/45/trace",
            required_header: Some(("private-token", "fixture-token")),
            body_contains: None,
            response_content_type: "text/plain",
            response_body: "build complete\n",
        }],
        CliCommand::RetryJob { .. } => job_action("retry"),
        CliCommand::CancelJob { .. } => job_action("cancel"),
        CliCommand::CancelRun { .. } => vec![
            get("/api/v4/projects/acme%2Fwidget/pipelines/92", PIPELINE),
            post("/api/v4/projects/acme%2Fwidget/pipelines/92/cancel", "{}"),
        ],
        CliCommand::OpenMr { .. } => vec![
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
        ],
        CliCommand::SetMr { .. } | CliCommand::EditMr { .. } => vec![
            get("/api/v4/projects/acme%2Fwidget/merge_requests/8", OPEN_MR),
            graphql(
                "GitlabUpdateMergeRequest",
                r#"{"data":{"mergeRequestUpdate":{"errors":[],"mergeRequest":{"id":"merge-request-8","iid":"8","webUrl":"https://gitlab.test/acme/widget/-/merge_requests/8"}}}}"#,
            ),
            get("/api/v4/projects/acme%2Fwidget/merge_requests/8", OPEN_MR),
        ],
        CliCommand::CommentMr { .. } => vec![
            get("/api/v4/projects/acme%2Fwidget/merge_requests/8", OPEN_MR),
            graphql("GitlabReadMergeRequestByIid", THREADS),
            graphql(
                "GitlabAddMergeRequestComment",
                r#"{"data":{"createNote":{"errors":[],"note":{"id":"note-2","url":null}}}}"#,
            ),
        ],
        CliCommand::ReplyThread { .. } => vec![
            get("/api/v4/projects/acme%2Fwidget/merge_requests/8", OPEN_MR),
            graphql("GitlabReadMergeRequestByIid", THREADS),
            graphql(
                "GitlabReplyToDiscussion",
                r#"{"data":{"createNote":{"errors":[],"note":{"id":"note-2","url":null}}}}"#,
            ),
        ],
        CliCommand::SetThread { .. } => vec![
            get("/api/v4/projects/acme%2Fwidget/merge_requests/8", OPEN_MR),
            graphql("GitlabReadMergeRequestByIid", THREADS),
            graphql(
                "GitlabSetDiscussionResolved",
                r#"{"data":{"discussionToggleResolve":{"errors":[],"discussion":{"id":"gid://gitlab/Discussion/thread-8","resolved":true}}}}"#,
            ),
        ],
        CliCommand::ReportWtToolBug { .. }
        | CliCommand::ReportWtToolIssue { .. }
        | CliCommand::SuggestWtToolImprovement { .. }
        | CliCommand::RequestWtToolFeature { .. } => unreachable!("not covered by this provider snapshot test"),
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
