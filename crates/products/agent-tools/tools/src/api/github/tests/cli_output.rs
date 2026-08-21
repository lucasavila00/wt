use super::*;
use crate::api::render_cli_command_output;

const PULL_REQUEST: &str = r#"{"number":7,"node_id":"pull-request-7","html_url":"https://github.test/acme/widget/pull/7","title":"Fix login","body":"Fixes the login flow.","state":"closed","draft":false,"head":{"ref":"wt/fix-login","sha":"abc123","repo":{"full_name":"acme/widget"}},"base":{"ref":"main","sha":"def456","repo":{"full_name":"acme/widget"}}}"#;
const WORKFLOW_RUN: &str = r#"{"id":91,"name":"CI","event":"pull_request","status":"completed","conclusion":"success","html_url":"https://github.test/runs/91","head_sha":"abc123","head_branch":"wt/fix-login","head_repository":{"full_name":"acme/widget"}}"#;
const WORKFLOW_JOB: &str = r#"{"id":44,"name":"Linux","status":"completed","conclusion":"success","html_url":"https://github.test/jobs/44","run_id":91}"#;
const REVIEW_THREADS: &str = r#"{"data":{"repository":{"pullRequest":{"reviewThreads":{"pageInfo":{"hasNextPage":false},"totalCount":1,"nodes":[{"id":"thread-7","isResolved":false,"path":"src/lib.rs","line":12,"comments":{"pageInfo":{"hasNextPage":false},"totalCount":1,"nodes":[{"author":{"__typename":"User","login":"reviewer"},"body":"Please clarify this.","url":"https://github.test/thread/7"}]}}]}}}}}"#;

#[test]
fn cli_commands_render_complete_json_from_github_responses() {
    let cases = vec![
        (
            "show_mr",
            CliCommand::ShowMr { mr: 7 },
            vec![get("/repos/acme/widget/pulls/7", PULL_REQUEST)],
        ),
        (
            "show_mr_for_branch",
            CliCommand::ShowMrForBranch {
                branch: "wt/fix-login".to_owned(),
            },
            vec![get(
                "/repos/acme/widget/pulls?state=open&head=acme%3Awt%2Ffix-login&per_page=100",
                leak(format!(
                    "[{}]",
                    PULL_REQUEST.replace(r#""state":"closed""#, r#""state":"open""#)
                )),
            )],
        ),
        (
            "show_run",
            CliCommand::ShowRun { run: 91 },
            vec![get("/repos/acme/widget/actions/runs/91", WORKFLOW_RUN)],
        ),
        (
            "show_job",
            CliCommand::ShowJob { job: 44 },
            vec![get("/repos/acme/widget/actions/jobs/44", WORKFLOW_JOB)],
        ),
        (
            "list_threads",
            CliCommand::ListThreads { mr: 7 },
            vec![graphql("GithubReadPullRequestByNumber", REVIEW_THREADS)],
        ),
        (
            "list_ci",
            CliCommand::ListCi {
                commit: "abc123".to_owned(),
            },
            vec![
                get(
                    "/repos/acme/widget/actions/runs?head_sha=abc123&per_page=100",
                    leak(format!(
                        r#"{{"total_count":1,"workflow_runs":[{WORKFLOW_RUN}]}}"#
                    )),
                ),
                get(
                    "/repos/acme/widget/actions/runs/91/jobs?filter=latest&per_page=100",
                    leak(format!(r#"{{"total_count":1,"jobs":[{WORKFLOW_JOB}]}}"#)),
                ),
            ],
        ),
        (
            "list_jobs",
            CliCommand::ListJobs { run: 91 },
            vec![get(
                "/repos/acme/widget/actions/runs/91/jobs?filter=latest&per_page=100",
                leak(format!(r#"{{"total_count":1,"jobs":[{WORKFLOW_JOB}]}}"#)),
            )],
        ),
        (
            "log_job",
            CliCommand::LogJob { job: 44 },
            vec![
                get("/repos/acme/widget/actions/jobs/44", WORKFLOW_JOB),
                get_text(
                    "/repos/acme/widget/actions/jobs/44/logs",
                    "build complete\n",
                ),
            ],
        ),
        (
            "wait_mr",
            CliCommand::WaitMr {
                mr: 7,
                timeout_seconds: None,
            },
            vec![get("/repos/acme/widget/pulls/7", PULL_REQUEST)],
        ),
        (
            "wait_run",
            CliCommand::WaitRun {
                run: 91,
                timeout_seconds: None,
            },
            vec![get("/repos/acme/widget/actions/runs/91", WORKFLOW_RUN)],
        ),
        (
            "wait_job",
            CliCommand::WaitJob {
                job: 44,
                timeout_seconds: None,
            },
            vec![get("/repos/acme/widget/actions/jobs/44", WORKFLOW_JOB)],
        ),
        (
            "open_mr",
            CliCommand::OpenMr {
                head: "wt/fix-login".to_owned(),
                base: "main".to_owned(),
                draft: false,
            },
            vec![
                get(
                    "/repos/acme/widget/commits/wt%2Ffix-login",
                    r#"{"sha":"abc123"}"#,
                ),
                graphql("GithubReadPullRequest", NO_PULL_REQUEST_RESPONSE),
                graphql(
                    "GithubCreatePullRequest",
                    r#"{"data":{"createPullRequest":{"pullRequest":{"id":"pull-request-7","number":7,"url":"https://github.test/acme/widget/pull/7","title":"Fix login","state":"OPEN","isDraft":false,"headRefOid":"abc123","baseRefName":"main","reviewDecision":null}}}}"#,
                ),
                graphql("GithubReadPullRequest", PULL_REQUEST_RESPONSE),
            ],
        ),
        (
            "set_mr",
            CliCommand::SetMr {
                mr: 7,
                state: ChangeRequestState::Ready,
            },
            vec![
                get("/repos/acme/widget/pulls/7", PULL_REQUEST),
                graphql(
                    "GithubMarkPullRequestReady",
                    r#"{"data":{"markPullRequestReadyForReview":{"pullRequest":{"id":"pull-request-7","number":7,"url":"https://github.test/acme/widget/pull/7","title":"Fix login","state":"OPEN","isDraft":false,"headRefOid":"abc123","baseRefName":"main","reviewDecision":null}}}}"#,
                ),
                get("/repos/acme/widget/pulls/7", PULL_REQUEST),
            ],
        ),
        (
            "edit_mr",
            CliCommand::EditMr {
                mr: 7,
                title: Some("Better title".to_owned()),
                body: None,
            },
            vec![
                get("/repos/acme/widget/pulls/7", PULL_REQUEST),
                graphql(
                    "GithubUpdatePullRequest",
                    r#"{"data":{"updatePullRequest":{"pullRequest":{"id":"pull-request-7","number":7,"url":"https://github.test/acme/widget/pull/7","title":"Better title","state":"OPEN","isDraft":false,"headRefOid":"abc123","baseRefName":"main","reviewDecision":null}}}}"#,
                ),
                get("/repos/acme/widget/pulls/7", PULL_REQUEST),
            ],
        ),
        (
            "comment_mr",
            CliCommand::CommentMr {
                mr: 7,
                body: "Done".to_owned(),
            },
            vec![
                get("/repos/acme/widget/pulls/7", PULL_REQUEST),
                graphql(
                    "GithubAddPullRequestComment",
                    r#"{"data":{"addComment":{"commentEdge":{"node":{"url":"https://github.test/comment/1"}}}}}"#,
                ),
            ],
        ),
        (
            "reply_thread",
            CliCommand::ReplyThread {
                mr: 7,
                thread: ReviewThreadHandle::new("thread-7"),
                body: "Done".to_owned(),
            },
            vec![
                get("/repos/acme/widget/pulls/7", PULL_REQUEST),
                graphql("GithubReadPullRequestByNumber", REVIEW_THREADS),
                graphql(
                    "GithubReplyToReviewThread",
                    r#"{"data":{"addPullRequestReviewThreadReply":{"comment":{"url":"https://github.test/comment/2"}}}}"#,
                ),
            ],
        ),
        (
            "set_thread",
            CliCommand::SetThread {
                mr: 7,
                thread: ReviewThreadHandle::new("thread-7"),
                resolved: true,
            },
            vec![
                get("/repos/acme/widget/pulls/7", PULL_REQUEST),
                graphql("GithubReadPullRequestByNumber", REVIEW_THREADS),
                graphql(
                    "GithubResolveReviewThread",
                    r#"{"data":{"resolveReviewThread":{"thread":{"id":"thread-7","isResolved":true}}}}"#,
                ),
            ],
        ),
        (
            "retry_job",
            CliCommand::RetryJob { job: 44 },
            vec![
                get("/repos/acme/widget/actions/jobs/44", WORKFLOW_JOB),
                get("/repos/acme/widget/actions/runs/91", WORKFLOW_RUN),
                post("/repos/acme/widget/actions/jobs/44/rerun"),
            ],
        ),
        (
            "cancel_run",
            CliCommand::CancelRun { run: 91 },
            vec![
                get("/repos/acme/widget/actions/runs/91", WORKFLOW_RUN),
                post("/repos/acme/widget/actions/runs/91/cancel"),
            ],
        ),
    ];

    for (name, command, requests) in cases {
        let (base_url, server) = serve(requests);
        let provider = GithubApi::with_base_url(base_url, "fixture-token").unwrap();
        let output = provider
            .execute_cli_command(&project_scope(), &command)
            .unwrap();
        let rendered = render_cli_command_output(output);
        let value: serde_json::Value = serde_json::from_str(&rendered).unwrap();

        insta::assert_snapshot!(
            format!("github_cli_json__{name}"),
            serde_json::to_string_pretty(&value).unwrap()
        );
        server.join().unwrap().unwrap();
    }
}

#[test]
fn cancel_job_reports_githubs_real_command_error() {
    let (base_url, server) = serve(vec![
        get("/repos/acme/widget/actions/jobs/44", WORKFLOW_JOB),
        get("/repos/acme/widget/actions/runs/91", WORKFLOW_RUN),
    ]);
    let provider = GithubApi::with_base_url(base_url, "fixture-token").unwrap();

    let error = provider
        .execute_cli_command(&project_scope(), &CliCommand::CancelJob { job: 44 })
        .unwrap_err();

    insta::assert_snapshot!(error);
    server.join().unwrap().unwrap();
}

fn get(path: &'static str, response_body: &'static str) -> ExpectedRequest {
    ExpectedRequest {
        method: "GET",
        path,
        required_header: Some(("authorization", "Bearer fixture-token")),
        body_contains: None,
        response_content_type: "application/json",
        response_body,
    }
}

fn get_text(path: &'static str, response_body: &'static str) -> ExpectedRequest {
    ExpectedRequest {
        response_content_type: "text/plain",
        ..get(path, response_body)
    }
}

fn graphql(operation: &'static str, response_body: &'static str) -> ExpectedRequest {
    ExpectedRequest {
        method: "POST",
        path: "/graphql",
        required_header: Some(("authorization", "Bearer fixture-token")),
        body_contains: Some(operation),
        response_content_type: "application/json",
        response_body,
    }
}

fn post(path: &'static str) -> ExpectedRequest {
    ExpectedRequest {
        method: "POST",
        path,
        required_header: Some(("authorization", "Bearer fixture-token")),
        body_contains: None,
        response_content_type: "application/json",
        response_body: "{}",
    }
}

fn leak(value: String) -> &'static str {
    Box::leak(value.into_boxed_str())
}
