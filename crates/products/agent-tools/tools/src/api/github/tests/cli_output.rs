use super::*;
use crate::api::render_cli_command_output;

const PULL_REQUEST: &str = r#"{"number":7,"node_id":"pull-request-7","html_url":"https://github.test/acme/widget/pull/7","title":"Fix login","body":"Fixes the login flow.","state":"closed","draft":false,"head":{"ref":"wt/fix-login","sha":"abc123","repo":{"full_name":"acme/widget"}},"base":{"ref":"main","sha":"def456","repo":{"full_name":"acme/widget"}},"mergeable":false}"#;
const WORKFLOW_RUN: &str = r#"{"id":91,"name":"CI","event":"pull_request","status":"completed","conclusion":"success","html_url":"https://github.test/runs/91","head_sha":"abc123","head_branch":"wt/fix-login","head_repository":{"full_name":"acme/widget"}}"#;
const WORKFLOW_JOB: &str = r#"{"id":44,"name":"Linux","status":"completed","conclusion":"success","html_url":"https://github.test/jobs/44","run_id":91}"#;
const REVIEW_THREADS: &str = r#"{"data":{"repository":{"pullRequest":{"reviewThreads":{"pageInfo":{"hasNextPage":false},"totalCount":1,"nodes":[{"id":"thread-7","isResolved":false,"path":"src/lib.rs","line":12,"comments":{"pageInfo":{"hasNextPage":false},"totalCount":1,"nodes":[{"author":{"__typename":"User","login":"reviewer"},"body":"Please clarify this.","url":"https://github.test/thread/7"}]}}]}}}}}"#;
const ISSUE_COMMENT: &str = r#"{"id":123,"body":"General feedback.","html_url":"https://github.test/acme/widget/pull/7#issuecomment-123","issue_url":"https://api.github.test/repos/acme/widget/issues/7","user":{"login":"reviewer"},"created_at":"2026-08-22T10:00:00Z","updated_at":"2026-08-22T11:00:00Z"}"#;

#[test]
fn cli_commands_render_complete_json_from_github_responses() {
    let cases = vec![
        (
            "show_mr",
            WtToolsCommand::ShowMr { mr: "7".into() },
            vec![
                get("/repos/acme/widget/pulls/7", PULL_REQUEST),
                get(
                    "/repos/acme/widget/actions/runs?head_sha=abc123&per_page=100",
                    r#"{"total_count":0,"workflow_runs":[]}"#,
                ),
            ],
        ),
        (
            "show_mr_for_branch",
            WtToolsCommand::ShowMrForBranch {
                branch: "wt/fix-login".to_owned(),
            },
            vec![
                get(
                    "/repos/acme/widget/pulls?state=open&head=acme%3Awt%2Ffix-login&per_page=100",
                    leak(format!(
                        "[{}]",
                        PULL_REQUEST.replace(r#""state":"closed""#, r#""state":"open""#)
                    )),
                ),
                get(
                    "/repos/acme/widget/pulls/7",
                    leak(PULL_REQUEST.replace(r#""state":"closed""#, r#""state":"open""#)),
                ),
                get(
                    "/repos/acme/widget/actions/runs?head_sha=abc123&per_page=100",
                    r#"{"total_count":0,"workflow_runs":[]}"#,
                ),
            ],
        ),
        (
            "show_run",
            WtToolsCommand::ShowRun { run: "91".into() },
            vec![get("/repos/acme/widget/actions/runs/91", WORKFLOW_RUN)],
        ),
        (
            "show_job",
            WtToolsCommand::ShowJob { job: "44".into() },
            vec![get("/repos/acme/widget/actions/jobs/44", WORKFLOW_JOB)],
        ),
        (
            "list_threads",
            WtToolsCommand::ListThreads { mr: "7".into() },
            vec![graphql("GithubReadPullRequestByNumber", REVIEW_THREADS)],
        ),
        (
            "list_comments",
            WtToolsCommand::ListComments { mr: "7".into() },
            vec![
                get("/repos/acme/widget/pulls/7", PULL_REQUEST),
                get(
                    "/repos/acme/widget/issues/7/comments?per_page=100&page=1",
                    leak(format!("[{ISSUE_COMMENT}]")),
                ),
            ],
        ),
        (
            "show_comment",
            WtToolsCommand::ShowComment {
                mr: "7".into(),
                comment: "123".into(),
            },
            vec![
                get("/repos/acme/widget/pulls/7", PULL_REQUEST),
                get("/repos/acme/widget/issues/comments/123", ISSUE_COMMENT),
            ],
        ),
        (
            "list_ci",
            WtToolsCommand::ListCi {
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
            WtToolsCommand::ListJobs { run: "91".into() },
            vec![get(
                "/repos/acme/widget/actions/runs/91/jobs?filter=latest&per_page=100",
                leak(format!(r#"{{"total_count":1,"jobs":[{WORKFLOW_JOB}]}}"#)),
            )],
        ),
        (
            "log_job",
            WtToolsCommand::LogJob { job: "44".into() },
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
            WtToolsCommand::WaitMr {
                mr: "7".into(),
                timeout_seconds: None,
            },
            vec![get("/repos/acme/widget/pulls/7", PULL_REQUEST)],
        ),
        (
            "wait_run",
            WtToolsCommand::WaitRun {
                run: "91".into(),
                timeout_seconds: None,
            },
            vec![get("/repos/acme/widget/actions/runs/91", WORKFLOW_RUN)],
        ),
        (
            "wait_job",
            WtToolsCommand::WaitJob {
                job: "44".into(),
                timeout_seconds: None,
            },
            vec![get("/repos/acme/widget/actions/jobs/44", WORKFLOW_JOB)],
        ),
        (
            "open_mr",
            WtToolsCommand::OpenMr {
                head: "wt/fix-login".to_owned(),
                base: "main".to_owned(),
            },
            vec![
                get(
                    "/repos/acme/widget/commits/wt%2Ffix-login",
                    r#"{"sha":"abc123"}"#,
                ),
                graphql("GithubReadPullRequest", NO_PULL_REQUEST_RESPONSE),
                graphql(
                    "GithubCreatePullRequest",
                    r#"{"data":{"createPullRequest":{"pullRequest":{"id":"pull-request-7","number":7,"url":"https://github.test/acme/widget/pull/7","title":"Fix login","state":"OPEN","isDraft":true,"headRefOid":"abc123","baseRefName":"main","reviewDecision":null}}}}"#,
                ),
                graphql(
                    "GithubReadPullRequest",
                    leak(PULL_REQUEST_RESPONSE.replace("\"isDraft\": false", "\"isDraft\": true")),
                ),
            ],
        ),
        (
            "set_mr",
            WtToolsCommand::SetMr {
                mr: "7".into(),
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
            WtToolsCommand::EditMr {
                mr: "7".into(),
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
            WtToolsCommand::CommentMr {
                mr: "7".into(),
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
            WtToolsCommand::ReplyThread {
                mr: "7".into(),
                thread: "thread-7".to_owned(),
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
            WtToolsCommand::SetThread {
                mr: "7".into(),
                thread: "thread-7".to_owned(),
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
            WtToolsCommand::RetryJob { job: "44".into() },
            vec![
                get("/repos/acme/widget/actions/jobs/44", WORKFLOW_JOB),
                get("/repos/acme/widget/actions/runs/91", WORKFLOW_RUN),
                post("/repos/acme/widget/actions/jobs/44/rerun"),
            ],
        ),
        (
            "cancel_run",
            WtToolsCommand::CancelRun { run: "91".into() },
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
        .execute_cli_command(
            &project_scope(),
            &WtToolsCommand::CancelJob { job: "44".into() },
        )
        .unwrap_err();

    insta::assert_snapshot!(error);
    server.join().unwrap().unwrap();
}

#[test]
fn show_comment_rejects_a_comment_from_another_pull_request() {
    let wrong_mr = ISSUE_COMMENT.replace("/issues/7", "/issues/8").leak();
    let (base_url, server) = serve(vec![
        get("/repos/acme/widget/pulls/7", PULL_REQUEST),
        get("/repos/acme/widget/issues/comments/123", wrong_mr),
    ]);
    let provider = GithubApi::with_base_url(base_url, "fixture-token").unwrap();

    let error = provider
        .execute_cli_command(
            &project_scope(),
            &WtToolsCommand::ShowComment {
                mr: "7".into(),
                comment: "123".into(),
            },
        )
        .unwrap_err();

    assert_eq!(error.to_string(), "comment 123 does not belong to MR 7");
    server.join().unwrap().unwrap();
}

#[test]
fn show_comment_accepts_canonical_repository_casing() {
    let scope = ProviderProjectScope {
        project: "Acme/Widget",
        ..project_scope()
    };
    let (base_url, server) = serve(vec![
        get("/repos/Acme/Widget/pulls/7", PULL_REQUEST),
        get("/repos/Acme/Widget/issues/comments/123", ISSUE_COMMENT),
    ]);
    let provider = GithubApi::with_base_url(base_url, "fixture-token").unwrap();

    provider
        .execute_cli_command(
            &scope,
            &WtToolsCommand::ShowComment {
                mr: "7".into(),
                comment: "123".into(),
            },
        )
        .unwrap();

    server.join().unwrap().unwrap();
}

#[test]
fn list_comments_reads_every_rest_page() {
    let first_page = format!("[{}]", vec![ISSUE_COMMENT; 100].join(",")).leak();
    let (base_url, server) = serve(vec![
        get("/repos/acme/widget/pulls/7", PULL_REQUEST),
        get(
            "/repos/acme/widget/issues/7/comments?per_page=100&page=1",
            first_page,
        ),
        get(
            "/repos/acme/widget/issues/7/comments?per_page=100&page=2",
            "[]",
        ),
    ]);
    let provider = GithubApi::with_base_url(base_url, "fixture-token").unwrap();

    let ProviderCommandOutput::GeneralComments(comments) = provider
        .execute_cli_command(
            &project_scope(),
            &WtToolsCommand::ListComments { mr: "7".into() },
        )
        .unwrap()
    else {
        panic!("expected general comments");
    };

    assert_eq!(comments.len(), 100);
    server.join().unwrap().unwrap();
}

#[test]
fn wait_timeouts_preserve_the_last_observed_state() {
    let cases = [
        (
            WtToolsCommand::WaitMr {
                mr: "7".into(),
                timeout_seconds: Some(0),
            },
            get(
                "/repos/acme/widget/pulls/7",
                leak(PULL_REQUEST.replace(r#""state":"closed""#, r#""state":"open""#)),
            ),
            "mr 7",
            "open",
        ),
        (
            WtToolsCommand::WaitJob {
                job: "44".into(),
                timeout_seconds: Some(0),
            },
            get(
                "/repos/acme/widget/actions/jobs/44",
                leak(
                    WORKFLOW_JOB
                        .replace(r#""status":"completed""#, r#""status":"in_progress""#)
                        .replace(r#""conclusion":"success""#, r#""conclusion":null"#),
                ),
            ),
            "job 44",
            "in_progress",
        ),
    ];

    for (command, request, resource, last_state) in cases {
        let (base_url, server) = serve(vec![request]);
        let provider = GithubApi::with_base_url(base_url, "fixture-token").unwrap();

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
    let response = leak(
        WORKFLOW_RUN
            .replace(r#""status":"completed""#, r#""status":"in_progress""#)
            .replace(r#""conclusion":"success""#, r#""conclusion":null"#),
    );
    let (base_url, server) = serve(vec![get("/repos/acme/widget/actions/runs/91", response)]);
    let provider = GithubApi::with_base_url(base_url, "fixture-token").unwrap();

    let output = provider
        .execute_cli_command(
            &project_scope(),
            &WtToolsCommand::WaitRun {
                run: "91".into(),
                timeout_seconds: Some(0),
            },
        )
        .unwrap();

    insta::assert_snapshot!(render_cli_command_output(output), @r###"
    {"type":"ci_run","data":{"handle":"91","name":"CI","state":"in_progress","trigger":"pull_request","url":"https://github.test/runs/91","head":"abc123","branch":"wt/fix-login"}}
    "###);
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
