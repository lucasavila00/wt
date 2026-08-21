use super::*;
use crate::api::test_server::{serve, ExpectedRequest};

mod branch_lookup;
mod cli_snapshots;

const MERGE_REQUEST_RESPONSE: &str = r#"{
    "data": {
        "currentUser": { "username": "agent" },
        "project": {
            "id": "project-1",
            "fullPath": "acme/widget",
            "userPermissions": { "createMergeRequestIn": true },
            "repository": { "commit": { "sha": "abc123" } },
            "mergeRequests": {
                "pageInfo": { "hasNextPage": false },
                "nodes": [{
                    "id": "merge-request-8",
                    "iid": "8",
                    "title": "Fix login",
                    "webUrl": "https://gitlab.test/acme/widget/-/merge_requests/8",
                    "state": "opened",
                    "draft": false,
                    "diffHeadSha": "abc123",
                    "sourceBranch": "df1/fix-login",
                    "targetBranch": "main",
                    "discussions": {
                        "pageInfo": { "hasNextPage": false },
                        "nodes": [{
                            "id": "discussion-1",
                            "resolved": false,
                            "resolvable": true,
                            "notes": {
                                "pageInfo": { "hasNextPage": false },
                                "nodes": [{
                                    "author": { "username": "reviewer" },
                                    "body": "Handle the error here.",
                                    "url": "https://gitlab.test/acme/widget/-/merge_requests/8#note_1",
                                    "position": {
                                        "filePath": "src/login.rs",
                                        "newLine": 42,
                                        "oldLine": null
                                    }
                                }]
                            }
                        }]
                    }
                }]
            }
        }
    }
}"#;

const NO_MERGE_REQUEST_RESPONSE: &str = r#"{
    "data": {
        "currentUser": { "username": "agent" },
        "project": {
            "id": "project-1",
            "fullPath": "acme/widget",
            "userPermissions": { "createMergeRequestIn": true },
            "repository": { "commit": { "sha": "abc123" } },
            "mergeRequests": {
                "pageInfo": { "hasNextPage": false },
                "nodes": []
            }
        }
    }
}"#;

const HISTORICAL_MERGE_REQUEST_RESPONSE: &str = r#"{
    "data": {
        "currentUser": { "username": "agent" },
        "project": {
            "id": "project-1",
            "fullPath": "acme/widget",
            "userPermissions": { "createMergeRequestIn": true },
            "repository": { "commit": { "sha": "abc123" } },
            "mergeRequests": {
                "pageInfo": { "hasNextPage": false },
                "nodes": [{
                    "id": "old-mr",
                    "iid": "7",
                    "title": "Old change",
                    "webUrl": "https://gitlab.test/acme/widget/-/merge_requests/7",
                    "state": "closed",
                    "draft": false,
                    "diffHeadSha": "old123",
                    "sourceBranch": "df1/fix-login",
                    "targetBranch": "main",
                    "discussions": {
                        "pageInfo": { "hasNextPage": false },
                        "nodes": []
                    }
                }]
            }
        }
    }
}"#;

const REORDERED_DISCUSSIONS_RESPONSE: &str = r#"{
    "data": {
        "currentUser": { "username": "agent" },
        "project": {
            "id": "project-1",
            "fullPath": "acme/widget",
            "userPermissions": { "createMergeRequestIn": true },
            "repository": { "commit": { "sha": "abc123" } },
            "mergeRequests": {
                "pageInfo": { "hasNextPage": false },
                "nodes": [{
                    "id": "merge-request-8",
                    "iid": "8",
                    "title": "Fix login",
                    "webUrl": "https://gitlab.test/acme/widget/-/merge_requests/8",
                    "state": "opened",
                    "draft": false,
                    "diffHeadSha": "abc123",
                    "sourceBranch": "df1/fix-login",
                    "targetBranch": "main",
                    "discussions": {
                        "pageInfo": { "hasNextPage": false },
                        "nodes": [
                            {
                                "id": "gid://gitlab/Discussion/fedcba654321-new",
                                "resolved": false,
                                "resolvable": true,
                                "notes": {
                                    "pageInfo": { "hasNextPage": false },
                                    "nodes": [{
                                        "author": { "username": "second-reviewer" },
                                        "body": "A newer thread.",
                                        "url": null
                                    }]
                                }
                            },
                            {
                                "id": "gid://gitlab/Discussion/abcdef123456-target",
                                "resolved": false,
                                "resolvable": true,
                                "notes": {
                                    "pageInfo": { "hasNextPage": false },
                                    "nodes": [{
                                        "author": { "username": "reviewer" },
                                        "body": "The target thread.",
                                        "url": null
                                    }]
                                }
                            },
                            {
                                "id": "gid://gitlab/Discussion/ordinary-note",
                                "resolved": false,
                                "resolvable": false,
                                "notes": {
                                    "pageInfo": { "hasNextPage": false },
                                    "nodes": [{
                                        "author": { "username": "author" },
                                        "body": "A normal MR comment.",
                                        "url": null
                                    }]
                                }
                            }
                        ]
                    }
                }]
            }
        }
    }
}"#;

#[test]
fn reads_merge_request_discussions_and_ci_from_local_fixture() {
    let (base_url, server) = serve(vec![
        ExpectedRequest {
            method: "POST",
            path: "/api/graphql",
            required_header: Some(("private-token", "fixture-token")),
            body_contains: Some("GitlabReadMergeRequest"),
            response_content_type: "application/json",
            response_body: MERGE_REQUEST_RESPONSE,
        },
        ExpectedRequest {
            method: "GET",
            path: "/api/v4/projects/acme%2Fwidget/merge_requests/8",
            required_header: Some(("private-token", "fixture-token")),
            body_contains: None,
            response_content_type: "application/json",
            response_body: r#"{"sha":"abc123","head_pipeline":{"id":92,"status":"success","web_url":"https://gitlab.test/pipelines/92","yaml_errors":null}}"#,
        },
        ExpectedRequest {
            method: "GET",
            path: "/api/v4/projects/acme%2Fwidget/pipelines/92/jobs?include_retried=false&per_page=100",
            required_header: Some(("private-token", "fixture-token")),
            body_contains: None,
            response_content_type: "application/json",
            response_body: r#"[{"id":45,"name":"test","status":"success","web_url":"https://gitlab.test/jobs/45"}]"#,
        },
    ]);
    let provider = GitlabApi::with_base_url(base_url, "fixture-token").unwrap();
    let scope = scope();

    let output = provider
        .execute_command(&scope, &ProviderCommand::ReadCurrentStatus)
        .unwrap();

    let ProviderCommandOutput::CurrentStatus(Some(request)) = output else {
        panic!("expected current merge request status");
    };
    assert_eq!(request.handle, "!8");
    assert_eq!(
        request.threads[0].handle,
        ReviewThreadHandle::new("discussion-1")
    );
    assert_eq!(request.threads[0].comments[0].author, "reviewer");
    assert_eq!(request.threads[0].path.as_deref(), Some("src/login.rs"));
    assert_eq!(request.threads[0].line, Some(42));
    assert_eq!(request.jobs[0].handle, CiJobHandle::new("45"));
    server.join().unwrap().unwrap();
}

#[test]
fn opens_merge_request_through_typed_graphql_mutation() {
    let (base_url, server) = serve(vec![
        ExpectedRequest {
            method: "POST",
            path: "/api/graphql",
            required_header: Some(("private-token", "fixture-token")),
            body_contains: Some("GitlabReadMergeRequest"),
            response_content_type: "application/json",
            response_body: NO_MERGE_REQUEST_RESPONSE,
        },
        ExpectedRequest {
            method: "POST",
            path: "/api/graphql",
            required_header: Some(("private-token", "fixture-token")),
            body_contains: Some("GitlabCreateMergeRequest"),
            response_content_type: "application/json",
            response_body: r#"{"data":{"mergeRequestCreate":{"errors":[],"mergeRequest":{"id":"merge-request-8","iid":"8","webUrl":"https://gitlab.test/acme/widget/-/merge_requests/8"}}}}"#,
        },
        ExpectedRequest {
            method: "POST",
            path: "/api/graphql",
            required_header: Some(("private-token", "fixture-token")),
            body_contains: Some("GitlabReadMergeRequest"),
            response_content_type: "application/json",
            response_body: MERGE_REQUEST_RESPONSE,
        },
    ]);
    let provider = GitlabApi::with_base_url(base_url, "fixture-token").unwrap();

    let output = provider
        .execute_command(
            &scope(),
            &ProviderCommand::OpenChangeRequest { draft: false },
        )
        .unwrap();

    let ProviderCommandOutput::ChangeRequest(request) = output else {
        panic!("expected opened merge request");
    };
    assert_eq!(request.handle, "!8");
    server.join().unwrap().unwrap();
}

#[test]
fn stable_review_handle_selects_its_discussion_after_reordering() {
    let (base_url, server) = serve(vec![
        ExpectedRequest {
            method: "POST",
            path: "/api/graphql",
            required_header: Some(("private-token", "fixture-token")),
            body_contains: Some("GitlabReadMergeRequest"),
            response_content_type: "application/json",
            response_body: REORDERED_DISCUSSIONS_RESPONSE,
        },
        ExpectedRequest {
            method: "POST",
            path: "/api/graphql",
            required_header: Some(("private-token", "fixture-token")),
            body_contains: Some("abcdef123456-target"),
            response_content_type: "application/json",
            response_body: r#"{"data":{"createNote":{"errors":[],"note":{"id":"note-2","url":null}}}}"#,
        },
    ]);
    let provider = GitlabApi::with_base_url(base_url, "fixture-token").unwrap();

    let output = provider
        .execute_command(
            &scope(),
            &ProviderCommand::ReplyToReviewThread {
                thread: ReviewThreadHandle::new("gid://gitlab/Discussion/abcdef123456-target"),
                body: "Fixed.".to_owned(),
            },
        )
        .unwrap();

    assert_eq!(
        output,
        ProviderCommandOutput::Confirmation("Reply added.".to_owned())
    );
    server.join().unwrap().unwrap();
}

#[test]
fn hides_non_resolvable_merge_request_notes_from_review_threads() {
    let (base_url, server) = serve(vec![ExpectedRequest {
        method: "POST",
        path: "/api/graphql",
        required_header: Some(("private-token", "fixture-token")),
        body_contains: Some("GitlabReadMergeRequest"),
        response_content_type: "application/json",
        response_body: REORDERED_DISCUSSIONS_RESPONSE,
    }]);
    let provider = GitlabApi::with_base_url(base_url, "fixture-token").unwrap();

    let output = provider
        .execute_command(&scope(), &ProviderCommand::ReadReviewThreads)
        .unwrap();

    let ProviderCommandOutput::ReviewThreads(threads) = output else {
        panic!("expected review threads");
    };
    assert_eq!(threads.len(), 2);
    assert!(threads
        .iter()
        .all(|thread| thread.comments[0].body != "A normal MR comment."));
    server.join().unwrap().unwrap();
}

#[test]
fn historical_merge_request_does_not_hide_a_pushed_current_branch() {
    let (base_url, server) = serve(vec![ExpectedRequest {
        method: "POST",
        path: "/api/graphql",
        required_header: Some(("private-token", "fixture-token")),
        body_contains: Some("GitlabReadMergeRequest"),
        response_content_type: "application/json",
        response_body: HISTORICAL_MERGE_REQUEST_RESPONSE,
    }]);
    let provider = GitlabApi::with_base_url(base_url, "fixture-token").unwrap();

    let output = provider
        .execute_command(&scope(), &ProviderCommand::ReadCurrentStatus)
        .unwrap();

    assert_eq!(output, ProviderCommandOutput::CurrentStatus(None));
    server.join().unwrap().unwrap();
}

#[test]
fn failed_pipeline_without_jobs_is_still_reported_as_failed_ci() {
    let (base_url, server) = serve(vec![
        ExpectedRequest {
            method: "POST",
            path: "/api/graphql",
            required_header: Some(("private-token", "fixture-token")),
            body_contains: Some("GitlabReadMergeRequest"),
            response_content_type: "application/json",
            response_body: MERGE_REQUEST_RESPONSE,
        },
        ExpectedRequest {
            method: "GET",
            path: "/api/v4/projects/acme%2Fwidget/merge_requests/8",
            required_header: Some(("private-token", "fixture-token")),
            body_contains: None,
            response_content_type: "application/json",
            response_body: r#"{"sha":"abc123","head_pipeline":{"id":92,"status":"failed","web_url":"https://gitlab.test/pipelines/92","yaml_errors":"invalid configuration"}}"#,
        },
        ExpectedRequest {
            method: "GET",
            path: "/api/v4/projects/acme%2Fwidget/pipelines/92/jobs?include_retried=false&per_page=100",
            required_header: Some(("private-token", "fixture-token")),
            body_contains: None,
            response_content_type: "application/json",
            response_body: "[]",
        },
    ]);
    let provider = GitlabApi::with_base_url(base_url, "fixture-token").unwrap();

    let output = provider
        .execute_command(&scope(), &ProviderCommand::ReadCurrentStatus)
        .unwrap();

    let ProviderCommandOutput::CurrentStatus(Some(request)) = output else {
        panic!("expected current merge request status");
    };
    assert_eq!(request.jobs.len(), 1);
    assert_eq!(request.jobs[0].state, "failed");
    assert_eq!(
        request.jobs[0].name,
        "pipeline configuration: invalid configuration"
    );
    server.join().unwrap().unwrap();
}

#[test]
fn refuses_incomplete_graphql_connections() {
    let (base_url, server) = serve(vec![ExpectedRequest {
        method: "POST",
        path: "/api/graphql",
        required_header: Some(("private-token", "fixture-token")),
        body_contains: Some("GitlabReadMergeRequest"),
        response_content_type: "application/json",
        response_body: r#"{
            "data": {
                "currentUser": { "username": "agent" },
                "project": {
                    "id": "project-1",
                    "fullPath": "acme/widget",
                    "userPermissions": { "createMergeRequestIn": true },
                    "repository": { "commit": { "sha": "abc123" } },
                    "mergeRequests": {
                        "pageInfo": { "hasNextPage": true },
                        "nodes": []
                    }
                }
            }
        }"#,
    }]);
    let provider = GitlabApi::with_base_url(base_url, "fixture-token").unwrap();

    let error = provider
        .execute_command(&scope(), &ProviderCommand::ReadCurrentStatus)
        .unwrap_err();

    assert_eq!(
        error.to_string(),
        "GitLab returned more than 100 merge requests for this branch; refusing to choose one from an incomplete result"
    );
    server.join().unwrap().unwrap();
}

#[test]
fn normalizes_gitlabs_canceled_spelling_for_shared_status_output() {
    assert_eq!(normalized_ci_state("canceled".to_owned()), "cancelled");
    assert_eq!(normalized_ci_state("failed".to_owned()), "failed");
}

#[test]
fn job_log_can_be_read_outside_the_current_commit() {
    let (base_url, server) = serve(vec![ExpectedRequest {
        method: "GET",
        path: "/api/v4/projects/acme%2Fwidget/jobs/94633136939/trace",
        required_header: Some(("private-token", "fixture-token")),
        body_contains: None,
        response_content_type: "text/plain",
        response_body: "build complete\n",
    }]);
    let provider = GitlabApi::with_base_url(base_url, "fixture-token").unwrap();

    let output = provider
        .execute_command(
            &scope(),
            &ProviderCommand::ReadCiJobLog {
                job: CiJobHandle::new("94633136939"),
            },
        )
        .unwrap();

    assert_eq!(
        output,
        ProviderCommandOutput::CiJobLog("build complete\n".to_owned())
    );
    server.join().unwrap().unwrap();
}

#[test]
fn explicit_resource_commands_do_not_need_checkout_context() {
    let (base_url, server) = serve(vec![
        ExpectedRequest {
            method: "GET",
            path: "/api/v4/projects/acme%2Fwidget/merge_requests/8",
            required_header: Some(("private-token", "fixture-token")),
            body_contains: None,
            response_content_type: "application/json",
            response_body: r#"{"iid":8,"title":"Fix login","description":"Fixes the login flow.","web_url":"https://gitlab.test/acme/widget/-/merge_requests/8","state":"opened","draft":false,"sha":"abc123","source_branch":"wt/fix-login","target_branch":"main","source_project_id":12,"target_project_id":12}"#,
        },
        ExpectedRequest {
            method: "GET",
            path: "/api/v4/projects/acme%2Fwidget/jobs/45",
            required_header: Some(("private-token", "fixture-token")),
            body_contains: None,
            response_content_type: "application/json",
            response_body: r#"{"id":45,"name":"test","status":"success","web_url":"https://gitlab.test/jobs/45","ref":"wt/fix-login","pipeline":{"id":92}}"#,
        },
    ]);
    let provider = GitlabApi::with_base_url(base_url, "fixture-token").unwrap();
    let scope = project_scope();

    let mr = provider
        .execute_cli_command(&scope, &CliCommand::ShowMr { mr: 8 })
        .unwrap();
    let job = provider
        .execute_cli_command(
            &scope,
            &CliCommand::WaitJob {
                job: 45,
                timeout_seconds: None,
            },
        )
        .unwrap();

    let ProviderCommandOutput::ChangeRequest(mr) = mr else {
        panic!("expected MR")
    };
    let ProviderCommandOutput::CiJob(job) = job else {
        panic!("expected job")
    };
    assert_eq!(mr.handle, "8");
    assert_eq!(mr.body.as_deref(), Some("Fixes the login flow."));
    assert_eq!(job.state, "success");
    server.join().unwrap().unwrap();
}

#[test]
fn lists_threads_by_merge_request_iid() {
    let (base_url, server) = serve(vec![ExpectedRequest {
        method: "POST",
        path: "/api/graphql",
        required_header: Some(("private-token", "fixture-token")),
        body_contains: Some("GitlabReadMergeRequestByIid"),
        response_content_type: "application/json",
        response_body: r#"{
            "data": {
                "project": {
                    "mergeRequest": {
                        "id": "gid://gitlab/MergeRequest/8",
                        "diffHeadSha": "abc123",
                        "discussions": {
                            "pageInfo": { "hasNextPage": false },
                            "nodes": [{
                                "id": "gid://gitlab/Discussion/thread-8",
                                "resolved": false,
                                "resolvable": true,
                                "notes": {
                                    "pageInfo": { "hasNextPage": false },
                                    "nodes": [{
                                        "author": { "username": "reviewer" },
                                        "body": "Please clarify this.",
                                        "url": "https://gitlab.test/thread/8",
                                        "position": {
                                            "filePath": "src/lib.rs",
                                            "newLine": 12,
                                            "oldLine": null
                                        }
                                    }]
                                }
                            }]
                        }
                    }
                }
            }
        }"#,
    }]);
    let provider = GitlabApi::with_base_url(base_url, "fixture-token").unwrap();

    let output = provider
        .execute_cli_command(&project_scope(), &CliCommand::ListThreads { mr: 8 })
        .unwrap();

    let ProviderCommandOutput::ReviewThreads(threads) = output else {
        panic!("expected review threads")
    };
    assert_eq!(
        threads[0].handle.as_str(),
        "gid://gitlab/Discussion/thread-8"
    );
    server.join().unwrap().unwrap();
}

#[test]
fn write_scope_comes_from_provider_resource_metadata() {
    let mut request = MergeRequest {
        iid: 8,
        title: String::new(),
        description: None,
        web_url: String::new(),
        state: "opened".to_owned(),
        draft: false,
        sha: "abc123".to_owned(),
        source_branch: "main".to_owned(),
        target_branch: "main".to_owned(),
        source_project_id: Some(12),
        target_project_id: Some(12),
    };
    assert!(GitlabApi::require_writable_merge_request(&project_scope(), &request).is_err());
    request.source_branch = "wt/fix".to_owned();
    assert!(GitlabApi::require_writable_merge_request(&project_scope(), &request).is_ok());
    request.source_project_id = Some(13);
    assert!(GitlabApi::require_writable_merge_request(&project_scope(), &request).is_err());
}

#[test]
fn retries_only_a_job_from_the_current_head_pipeline() {
    let (base_url, server) = serve(vec![
        ExpectedRequest {
            method: "POST",
            path: "/api/graphql",
            required_header: Some(("private-token", "fixture-token")),
            body_contains: Some("GitlabReadMergeRequest"),
            response_content_type: "application/json",
            response_body: MERGE_REQUEST_RESPONSE,
        },
        ExpectedRequest {
            method: "GET",
            path: "/api/v4/projects/acme%2Fwidget/merge_requests/8",
            required_header: Some(("private-token", "fixture-token")),
            body_contains: None,
            response_content_type: "application/json",
            response_body: r#"{"sha":"abc123","head_pipeline":{"id":92,"status":"failed","web_url":"https://gitlab.test/pipelines/92","yaml_errors":null}}"#,
        },
        ExpectedRequest {
            method: "GET",
            path: "/api/v4/projects/acme%2Fwidget/pipelines/92/jobs?include_retried=false&per_page=100",
            required_header: Some(("private-token", "fixture-token")),
            body_contains: None,
            response_content_type: "application/json",
            response_body: r#"[{"id":45,"name":"test","status":"failed","web_url":"https://gitlab.test/jobs/45"}]"#,
        },
        ExpectedRequest {
            method: "POST",
            path: "/api/v4/projects/acme%2Fwidget/jobs/45/retry",
            required_header: Some(("private-token", "fixture-token")),
            body_contains: None,
            response_content_type: "application/json",
            response_body: "{}",
        },
    ]);
    let provider = GitlabApi::with_base_url(base_url, "fixture-token").unwrap();

    let output = provider
        .execute_command(
            &scope(),
            &ProviderCommand::RetryCiJob {
                job: CiJobHandle::new("45"),
            },
        )
        .unwrap();

    assert_eq!(
        output,
        ProviderCommandOutput::Confirmation("Retry requested for job 45.".to_owned())
    );
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

fn project_scope() -> ProviderProjectScope<'static> {
    ProviderProjectScope {
        host: "gitlab.test",
        project: "acme/widget",
        prefix: "wt/",
    }
}
