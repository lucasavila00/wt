use super::*;
use crate::api::test_server::{serve, serve_with_statuses, ExpectedRequest};

mod branch_lookup;
mod job_logs;

const PULL_REQUEST_RESPONSE: &str = r#"{
    "data": {
        "viewer": { "login": "agent" },
        "repository": {
            "id": "repository-1",
            "nameWithOwner": "acme/widget",
            "viewerPermission": "WRITE",
            "pullRequests": {
                "pageInfo": { "hasNextPage": false },
                "totalCount": 1,
                "nodes": [{
                    "id": "pull-request-7",
                    "number": 7,
                    "url": "https://github.test/acme/widget/pull/7",
                    "title": "Fix login",
                    "state": "OPEN",
                    "isDraft": false,
                    "headRefOid": "abc123",
                    "headRepository": { "id": "repository-1", "nameWithOwner": "acme/widget" },
                    "isCrossRepository": false,
                    "baseRefName": "main",
                    "reviewDecision": "CHANGES_REQUESTED",
                    "reviewThreads": {
                        "pageInfo": { "hasNextPage": false },
                        "totalCount": 1,
                        "nodes": [{
                            "id": "thread-1",
                            "isResolved": false,
                            "path": "src/login.rs",
                            "line": 12,
                            "comments": {
                                "pageInfo": { "hasNextPage": false },
                                "totalCount": 1,
                                "nodes": [{
                                    "author": { "__typename": "User", "login": "reviewer" },
                                    "body": "Handle the error here.",
                                    "url": "https://github.test/acme/widget/pull/7#discussion"
                                }]
                            }
                        }]
                    },
                    "reviews": {
                        "pageInfo": { "hasNextPage": false },
                        "totalCount": 1,
                        "nodes": [{
                            "id": "review-1",
                            "author": { "__typename": "User", "login": "lead" },
                            "body": "Please cover the edge case.",
                            "state": "CHANGES_REQUESTED",
                            "url": "https://github.test/acme/widget/pull/7#review"
                        }]
                    },
                    "comments": {
                        "pageInfo": { "hasNextPage": false },
                        "totalCount": 1,
                        "nodes": [{
                            "id": "comment-1",
                            "author": { "__typename": "User", "login": "maintainer" },
                            "body": "Please update the documentation too.",
                            "url": "https://github.test/acme/widget/pull/7#issuecomment-1"
                        }]
                    },
                    "statusCheckRollup": {
                        "contexts": {
                            "pageInfo": { "hasNextPage": false },
                            "totalCount": 2,
                            "nodes": [
                                {
                                    "__typename": "CheckRun",
                                    "id": "check-44",
                                    "databaseId": 44,
                                    "name": "test",
                                    "status": "COMPLETED",
                                    "conclusion": "SUCCESS",
                                    "detailsUrl": "https://github.test/checks/44"
                                },
                                {
                                    "__typename": "StatusContext",
                                    "id": "status-1",
                                    "context": "external/lint",
                                    "state": "PENDING",
                                    "targetUrl": "https://ci.test/lint"
                                }
                            ]
                        }
                    }
                }]
            }
        }
    }
}"#;

const NO_PULL_REQUEST_RESPONSE: &str = r#"{
    "data": {
        "viewer": { "login": "agent" },
        "repository": {
            "id": "repository-1",
            "nameWithOwner": "acme/widget",
            "viewerPermission": "WRITE",
            "pullRequests": {
                "pageInfo": { "hasNextPage": false },
                "totalCount": 0,
                "nodes": []
            }
        }
    }
}"#;

#[test]
fn reads_pull_request_reviews_and_ci_from_local_fixture() {
    let (base_url, server) = serve(vec![
        ExpectedRequest {
            method: "POST",
            path: "/graphql",
            required_header: Some(("authorization", "Bearer fixture-token")),
            body_contains: Some("GithubReadPullRequest"),
            response_content_type: "application/json",
            response_body: PULL_REQUEST_RESPONSE,
        },
        ExpectedRequest {
            method: "GET",
            path: "/repos/acme/widget/actions/runs?head_sha=abc123&per_page=100",
            required_header: Some(("authorization", "Bearer fixture-token")),
            body_contains: None,
            response_content_type: "application/json",
            response_body: r#"{"total_count":1,"workflow_runs":[{"id":91}]}"#,
        },
        ExpectedRequest {
            method: "GET",
            path: "/repos/acme/widget/actions/runs/91/jobs?filter=latest&per_page=100",
            required_header: Some(("authorization", "Bearer fixture-token")),
            body_contains: None,
            response_content_type: "application/json",
            response_body: r#"{"total_count":1,"jobs":[{"id":44,"name":"test","status":"completed","conclusion":"success","html_url":"https://github.test/jobs/44","run_id":91}]}"#,
        },
    ]);
    let provider = GithubApi::with_base_url(base_url, "fixture-token").unwrap();
    let scope = scope();

    let output = provider
        .execute_command(&scope, &ProviderCommand::ReadCurrentStatus)
        .unwrap();

    let ProviderCommandOutput::CurrentStatus(Some(request)) = output else {
        panic!("expected current pull request status");
    };
    assert_eq!(request.handle, "#7");
    assert_eq!(request.threads[0].comments[0].author, "reviewer");
    assert_eq!(request.threads[0].handle.as_str(), "thread-1");
    assert_eq!(request.threads[1].handle.as_str(), "review-1");
    assert_eq!(request.threads[1].comments[0].author, "lead");
    assert_eq!(request.threads[2].handle.as_str(), "comment-1");
    assert_eq!(request.threads[2].comments[0].author, "maintainer");
    assert_eq!(request.jobs[0].handle, CiJobHandle::new("44"));
    assert_eq!(request.jobs[1].handle, CiJobHandle::new("S:status-1"));
    server.join().unwrap().unwrap();
}

#[test]
fn refuses_cross_repository_pull_request_even_when_branch_and_commit_match() {
    let response = leak_fixture(PULL_REQUEST_RESPONSE.replace(
        "\"isCrossRepository\": false",
        "\"isCrossRepository\": true",
    ));
    let (base_url, server) = serve(vec![graphql_fixture(response)]);
    let provider = GithubApi::with_base_url(base_url, "fixture-token").unwrap();

    let snapshot = provider
        .read_change_request_snapshot(&scope(), false)
        .unwrap();

    assert!(snapshot.request.is_none());
    server.join().unwrap().unwrap();
}

#[test]
fn merged_pull_request_remains_visible() {
    let response =
        leak_fixture(PULL_REQUEST_RESPONSE.replace("\"state\": \"OPEN\"", "\"state\": \"MERGED\""));
    let (base_url, server) = serve(vec![graphql_fixture(response)]);
    let provider = GithubApi::with_base_url(base_url, "fixture-token").unwrap();

    let snapshot = provider
        .read_change_request_snapshot(&scope(), false)
        .unwrap();

    assert_eq!(snapshot.request.unwrap().state, "merged");
    server.join().unwrap().unwrap();
}

#[test]
fn refuses_truncated_graphql_connections_instead_of_issuing_unsafe_handles() {
    let response = leak_fixture(PULL_REQUEST_RESPONSE.replacen(
        "\"hasNextPage\": false",
        "\"hasNextPage\": true",
        1,
    ));
    let (base_url, server) = serve(vec![graphql_fixture(response)]);
    let provider = GithubApi::with_base_url(base_url, "fixture-token").unwrap();

    let error = provider
        .read_change_request_snapshot(&scope(), false)
        .unwrap_err();

    assert_eq!(
        error.to_string(),
        "GitHub returned only the first page of pull requests (1 total); wt-git-hosting refuses to continue with incomplete handles or status"
    );
    server.join().unwrap().unwrap();
}

#[test]
fn stable_review_handle_selects_provider_thread_after_reordering() {
    let (base_url, server) = serve(vec![graphql_fixture(PULL_REQUEST_RESPONSE)]);
    let provider = GithubApi::with_base_url(base_url, "fixture-token").unwrap();
    let mut snapshot = provider
        .read_change_request_snapshot(&scope(), false)
        .unwrap();
    snapshot.review_targets.reverse();

    let target = GithubApi::review_target(&snapshot, &ReviewThreadHandle::new("thread-1")).unwrap();

    assert_eq!(
        target,
        GithubReviewTarget::Thread(GithubReviewThreadId("thread-1".to_owned()))
    );
    server.join().unwrap().unwrap();
}

#[test]
fn paginates_actions_results() {
    let (base_url, server) = serve(vec![
        ExpectedRequest {
            method: "GET",
            path: "/repos/acme/widget/actions/runs?head_sha=abc123&per_page=100",
            required_header: Some(("authorization", "Bearer fixture-token")),
            body_contains: None,
            response_content_type: "application/json",
            response_body: r#"{"total_count":2,"workflow_runs":[{"id":91}]}"#,
        },
        ExpectedRequest {
            method: "GET",
            path: "/repos/acme/widget/actions/runs?head_sha=abc123&per_page=100&page=2",
            required_header: Some(("authorization", "Bearer fixture-token")),
            body_contains: None,
            response_content_type: "application/json",
            response_body: r#"{"total_count":2,"workflow_runs":[{"id":92}]}"#,
        },
        ExpectedRequest {
            method: "GET",
            path: "/repos/acme/widget/actions/runs/91/jobs?filter=latest&per_page=100",
            required_header: Some(("authorization", "Bearer fixture-token")),
            body_contains: None,
            response_content_type: "application/json",
            response_body: r#"{"total_count":0,"jobs":[]}"#,
        },
        ExpectedRequest {
            method: "GET",
            path: "/repos/acme/widget/actions/runs/92/jobs?filter=latest&per_page=100",
            required_header: Some(("authorization", "Bearer fixture-token")),
            body_contains: None,
            response_content_type: "application/json",
            response_body: r#"{"total_count":0,"jobs":[]}"#,
        },
    ]);
    let provider = GithubApi::with_base_url(base_url, "fixture-token").unwrap();

    assert!(provider.read_action_jobs(&scope()).unwrap().is_empty());
    server.join().unwrap().unwrap();
}

#[test]
fn refuses_to_turn_job_cancellation_into_whole_run_cancellation() {
    let (base_url, server) = serve(vec![
        ExpectedRequest {
            method: "GET",
            path: "/repos/acme/widget/actions/runs?head_sha=abc123&per_page=100",
            required_header: Some(("authorization", "Bearer fixture-token")),
            body_contains: None,
            response_content_type: "application/json",
            response_body: r#"{"total_count":1,"workflow_runs":[{"id":91}]}"#,
        },
        ExpectedRequest {
            method: "GET",
            path: "/repos/acme/widget/actions/runs/91/jobs?filter=latest&per_page=100",
            required_header: Some(("authorization", "Bearer fixture-token")),
            body_contains: None,
            response_content_type: "application/json",
            response_body: r#"{"total_count":1,"jobs":[{"id":44,"name":"test","status":"in_progress","conclusion":null,"html_url":"https://github.test/jobs/44","run_id":91}]}"#,
        },
    ]);
    let provider = GithubApi::with_base_url(base_url, "fixture-token").unwrap();

    let error = provider
        .execute_command(
            &scope(),
            &ProviderCommand::CancelCiJob {
                job: CiJobHandle::new("44"),
            },
        )
        .unwrap_err();

    assert!(error
        .to_string()
        .contains("only cancel the entire workflow run"));
    server.join().unwrap().unwrap();
}

#[test]
fn explicit_resource_commands_do_not_need_checkout_context() {
    let (base_url, server) = serve(vec![
        ExpectedRequest {
            method: "GET",
            path: "/repos/acme/widget/pulls/7",
            required_header: Some(("authorization", "Bearer fixture-token")),
            body_contains: None,
            response_content_type: "application/json",
            response_body: r#"{"number":7,"node_id":"pull-request-7","html_url":"https://github.test/acme/widget/pull/7","title":"Fix login","state":"open","draft":false,"head":{"ref":"wt/fix-login","sha":"abc123","repo":{"full_name":"acme/widget"}},"base":{"ref":"main","sha":"def456","repo":{"full_name":"acme/widget"}}}"#,
        },
        ExpectedRequest {
            method: "GET",
            path: "/repos/acme/widget/actions/jobs/44",
            required_header: Some(("authorization", "Bearer fixture-token")),
            body_contains: None,
            response_content_type: "application/json",
            response_body: r#"{"id":44,"name":"Linux","status":"completed","conclusion":"success","html_url":"https://github.test/jobs/44","run_id":91}"#,
        },
    ]);
    let provider = GithubApi::with_base_url(base_url, "fixture-token").unwrap();
    let scope = project_scope();

    let mr = provider
        .execute_cli_command(&scope, &CliCommand::ShowMr { mr: 7 })
        .unwrap();
    let job = provider
        .execute_cli_command(&scope, &CliCommand::WaitJob { job: 44 })
        .unwrap();

    let ProviderCommandOutput::ChangeRequest(mr) = mr else {
        panic!("expected MR")
    };
    let ProviderCommandOutput::CiJob(job) = job else {
        panic!("expected job")
    };
    assert_eq!(mr.handle, "7");
    assert_eq!(job.state, "success");
    server.join().unwrap().unwrap();
}

#[test]
fn lists_threads_by_pull_request_number() {
    let (base_url, server) = serve(vec![ExpectedRequest {
        method: "POST",
        path: "/graphql",
        required_header: Some(("authorization", "Bearer fixture-token")),
        body_contains: Some("GithubReadPullRequestByNumber"),
        response_content_type: "application/json",
        response_body: r#"{
            "data": {
                "repository": {
                    "pullRequest": {
                        "reviewThreads": {
                            "pageInfo": { "hasNextPage": false },
                            "totalCount": 1,
                            "nodes": [{
                                "id": "PRRT_thread_7",
                                "isResolved": false,
                                "path": "src/lib.rs",
                                "line": 12,
                                "comments": {
                                    "pageInfo": { "hasNextPage": false },
                                    "totalCount": 1,
                                    "nodes": [{
                                        "author": { "__typename": "User", "login": "reviewer" },
                                        "body": "Please clarify this.",
                                        "url": "https://github.test/thread/7"
                                    }]
                                }
                            }]
                        }
                    }
                }
            }
        }"#,
    }]);
    let provider = GithubApi::with_base_url(base_url, "fixture-token").unwrap();

    let output = provider
        .execute_cli_command(&project_scope(), &CliCommand::ListThreads { mr: 7 })
        .unwrap();

    let ProviderCommandOutput::ReviewThreads(threads) = output else {
        panic!("expected review threads")
    };
    assert_eq!(threads[0].handle.as_str(), "PRRT_thread_7");
    server.join().unwrap().unwrap();
}

#[test]
fn refuses_a_thread_handle_from_another_pull_request() {
    let (base_url, server) = serve(vec![
        ExpectedRequest {
            method: "GET",
            path: "/repos/acme/widget/pulls/7",
            required_header: Some(("authorization", "Bearer fixture-token")),
            body_contains: None,
            response_content_type: "application/json",
            response_body: r#"{"number":7,"node_id":"pull-request-7","html_url":"https://github.test/acme/widget/pull/7","title":"Fix login","state":"open","draft":false,"head":{"ref":"wt/fix-login","sha":"abc123","repo":{"full_name":"acme/widget"}},"base":{"ref":"main","sha":"def456","repo":{"full_name":"acme/widget"}}}"#,
        },
        ExpectedRequest {
            method: "POST",
            path: "/graphql",
            required_header: Some(("authorization", "Bearer fixture-token")),
            body_contains: Some("GithubReadPullRequestByNumber"),
            response_content_type: "application/json",
            response_body: r#"{"data":{"repository":{"pullRequest":{"reviewThreads":{"pageInfo":{"hasNextPage":false},"totalCount":1,"nodes":[{"id":"thread-7","isResolved":false,"path":"src/lib.rs","line":12,"comments":{"pageInfo":{"hasNextPage":false},"totalCount":0,"nodes":[]}}]}}}}}"#,
        },
    ]);
    let provider = GithubApi::with_base_url(base_url, "fixture-token").unwrap();

    let error = provider
        .execute_cli_command(
            &project_scope(),
            &CliCommand::ReplyThread {
                mr: 7,
                thread: ReviewThreadHandle::new("thread-from-another-mr"),
                body: "No".to_owned(),
            },
        )
        .unwrap_err();

    assert_eq!(
        error.to_string(),
        "review thread `thread-from-another-mr` does not belong to MR 7"
    );
    server.join().unwrap().unwrap();
}

#[test]
fn write_scope_comes_from_provider_resource_metadata() {
    let mut request = PullRequest {
        number: 7,
        node_id: "pull-request-7".to_owned(),
        html_url: String::new(),
        title: String::new(),
        state: "open".to_owned(),
        draft: false,
        head: PullRequestRef {
            reference: "main".to_owned(),
            sha: "abc123".to_owned(),
            repo: Some(PullRequestRepository {
                full_name: "acme/widget".to_owned(),
            }),
        },
        base: PullRequestRef {
            reference: "main".to_owned(),
            sha: "def456".to_owned(),
            repo: Some(PullRequestRepository {
                full_name: "acme/widget".to_owned(),
            }),
        },
    };
    assert!(GithubApi::require_writable_pull_request(&project_scope(), &request).is_err());
    request.head.reference = "wt/fix".to_owned();
    assert!(GithubApi::require_writable_pull_request(&project_scope(), &request).is_ok());
}

#[test]
fn ci_write_scope_requires_the_selected_repository() {
    let mut run = WorkflowRun {
        id: 91,
        name: String::new(),
        status: String::new(),
        conclusion: None,
        html_url: None,
        head_sha: "abc123".to_owned(),
        head_branch: Some("wt/fix".to_owned()),
        head_repository: Some(WorkflowRunRepository {
            full_name: "acme/widget".to_owned(),
        }),
    };
    assert!(GithubApi::require_writable_run(&project_scope(), &run).is_ok());

    run.head_repository = Some(WorkflowRunRepository {
        full_name: "fork/widget".to_owned(),
    });
    assert!(GithubApi::require_writable_run(&project_scope(), &run).is_err());
}

#[test]
fn opens_pull_request_through_typed_graphql_mutation() {
    let (base_url, server) = serve(vec![
        ExpectedRequest {
            method: "POST",
            path: "/graphql",
            required_header: Some(("authorization", "Bearer fixture-token")),
            body_contains: Some("GithubReadPullRequest"),
            response_content_type: "application/json",
            response_body: NO_PULL_REQUEST_RESPONSE,
        },
        ExpectedRequest {
            method: "POST",
            path: "/graphql",
            required_header: Some(("authorization", "Bearer fixture-token")),
            body_contains: Some("GithubCreatePullRequest"),
            response_content_type: "application/json",
            response_body: r#"{"data":{"createPullRequest":{"pullRequest":{"id":"pull-request-7","number":7,"url":"https://github.test/acme/widget/pull/7","title":"Fix login","state":"OPEN","isDraft":false,"headRefOid":"abc123","baseRefName":"main","reviewDecision":null}}}}"#,
        },
        ExpectedRequest {
            method: "POST",
            path: "/graphql",
            required_header: Some(("authorization", "Bearer fixture-token")),
            body_contains: Some("GithubReadPullRequest"),
            response_content_type: "application/json",
            response_body: PULL_REQUEST_RESPONSE,
        },
    ]);
    let provider = GithubApi::with_base_url(base_url, "fixture-token").unwrap();

    let output = provider
        .execute_command(
            &scope(),
            &ProviderCommand::OpenChangeRequest { draft: false },
        )
        .unwrap();

    let ProviderCommandOutput::ChangeRequest(request) = output else {
        panic!("expected opened pull request");
    };
    assert_eq!(request.handle, "#7");
    server.join().unwrap().unwrap();
}

fn scope() -> ProviderCommandScope<'static> {
    ProviderCommandScope {
        project: "acme/widget",
        base: "main",
        prefix: "df1/",
        branch: "df1/fix-login",
        head: "abc123",
    }
}

fn graphql_fixture(response_body: &'static str) -> ExpectedRequest {
    ExpectedRequest {
        method: "POST",
        path: "/graphql",
        required_header: Some(("authorization", "Bearer fixture-token")),
        body_contains: Some("GithubReadPullRequest"),
        response_content_type: "application/json",
        response_body,
    }
}

fn leak_fixture(value: String) -> &'static str {
    Box::leak(value.into_boxed_str())
}

fn project_scope() -> ProviderProjectScope<'static> {
    ProviderProjectScope {
        host: "github.test",
        project: "acme/widget",
        prefix: "wt/",
    }
}
