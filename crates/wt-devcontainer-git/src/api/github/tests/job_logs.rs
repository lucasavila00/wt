use super::*;

#[test]
fn running_job_log_can_be_read_outside_the_current_commit() {
    let (base_url, server) = serve_with_statuses(vec![
        (
            ExpectedRequest {
                method: "GET",
                path: "/repos/acme/widget/actions/jobs/94318091035",
                required_header: Some(("authorization", "Bearer fixture-token")),
                body_contains: None,
                response_content_type: "application/json",
                response_body: r#"{"id":94318091035,"name":"Linux","status":"in_progress","conclusion":null,"html_url":"https://github.test/jobs/94318091035","run_id":91}"#,
            },
            200,
        ),
        (
            ExpectedRequest {
                method: "GET",
                path: "/repos/acme/widget/actions/jobs/94318091035/logs",
                required_header: Some(("authorization", "Bearer fixture-token")),
                body_contains: None,
                response_content_type: "application/xml",
                response_body: "<Error><Code>BlobNotFound</Code></Error>",
            },
            404,
        ),
    ]);
    let provider = GithubApi::with_base_url(base_url, "fixture-token").unwrap();
    let output = provider
        .execute_command(
            &scope(),
            &ProviderCommand::ReadCiJobLog {
                job: CiJobHandle::new("94318091035"),
            },
        )
        .unwrap();

    let ProviderCommandOutput::CiJobLog(output) = output else {
        panic!("expected a CI job log")
    };
    insta::assert_snapshot!(output, @r###"
    Job: 94318091035 (Linux)
    State: in_progress
    Log: GitHub has not published live log bytes for this running job.
    "###);
    server.join().unwrap().unwrap();
}

#[test]
fn completed_job_without_log_returns_check_run_diagnostics() {
    let (base_url, server) = serve_with_statuses(vec![
        (
            ExpectedRequest {
                method: "GET",
                path: "/repos/acme/widget/actions/jobs/95206818032",
                required_header: Some(("authorization", "Bearer fixture-token")),
                body_contains: None,
                response_content_type: "application/json",
                response_body: r#"{"id":95206818032,"name":"checks","status":"completed","conclusion":"failure","html_url":"https://github.test/jobs/95206818032","run_id":31964236640}"#,
            },
            200,
        ),
        (
            ExpectedRequest {
                method: "GET",
                path: "/repos/acme/widget/actions/jobs/95206818032/logs",
                required_header: Some(("authorization", "Bearer fixture-token")),
                body_contains: None,
                response_content_type: "application/json",
                response_body: r#"{"message":"Not Found"}"#,
            },
            404,
        ),
        (
            ExpectedRequest {
                method: "GET",
                path: "/repos/acme/widget/check-runs/95206818032/annotations?per_page=100&page=1",
                required_header: Some(("authorization", "Bearer fixture-token")),
                body_contains: None,
                response_content_type: "application/json",
                response_body: r#"[{"path":".github","start_line":1,"end_line":1,"annotation_level":"failure","title":"","message":"The job was not started because recent account payments have failed or your spending limit needs to be increased.","raw_details":""}]"#,
            },
            200,
        ),
    ]);
    let provider = GithubApi::with_base_url(base_url, "fixture-token").unwrap();

    let output = provider
        .execute_command(
            &scope(),
            &ProviderCommand::ReadCiJobLog {
                job: CiJobHandle::new("95206818032"),
            },
        )
        .unwrap();

    let ProviderCommandOutput::CiJobLog(output) = output else {
        panic!("expected a CI job log")
    };
    insta::assert_snapshot!(output, @r###"
    Job: 95206818032 (checks)
    State: failure
    Log: GitHub did not publish log bytes for this job.
    Diagnostics:
    - [failure] .github:1
      The job was not started because recent account payments have failed or your spending limit needs to be increased.
    Next step: ask the user to resolve this provider-side failure.
    "###);
    server.join().unwrap().unwrap();
}

#[test]
fn completed_job_without_log_or_annotations_asks_the_user_for_help() {
    let (base_url, server) = serve_with_statuses(vec![
        (
            ExpectedRequest {
                method: "GET",
                path: "/repos/acme/widget/actions/jobs/44",
                required_header: Some(("authorization", "Bearer fixture-token")),
                body_contains: None,
                response_content_type: "application/json",
                response_body: r#"{"id":44,"name":"checks","status":"completed","conclusion":"failure","html_url":"https://github.test/jobs/44","run_id":91}"#,
            },
            200,
        ),
        (
            ExpectedRequest {
                method: "GET",
                path: "/repos/acme/widget/actions/jobs/44/logs",
                required_header: Some(("authorization", "Bearer fixture-token")),
                body_contains: None,
                response_content_type: "application/json",
                response_body: r#"{"message":"Not Found"}"#,
            },
            404,
        ),
        (
            ExpectedRequest {
                method: "GET",
                path: "/repos/acme/widget/check-runs/44/annotations?per_page=100&page=1",
                required_header: Some(("authorization", "Bearer fixture-token")),
                body_contains: None,
                response_content_type: "application/json",
                response_body: "[]",
            },
            200,
        ),
    ]);
    let provider = GithubApi::with_base_url(base_url, "fixture-token").unwrap();

    let output = provider
        .execute_command(
            &scope(),
            &ProviderCommand::ReadCiJobLog {
                job: CiJobHandle::new("44"),
            },
        )
        .unwrap();

    let ProviderCommandOutput::CiJobLog(output) = output else {
        panic!("expected a CI job log")
    };
    insta::assert_snapshot!(output, @r###"
    Job: 44 (checks)
    State: failure
    Log: GitHub did not publish log bytes for this job.
    Diagnostics: GitHub reported no check annotations.
    Next step: ask the user to resolve this provider-side failure.
    "###);
    assert!(!output.contains("https://github"));
    server.join().unwrap().unwrap();
}

#[test]
fn completed_job_log_is_downloaded() {
    let (base_url, server) = serve(vec![
        ExpectedRequest {
            method: "GET",
            path: "/repos/acme/widget/actions/jobs/44",
            required_header: Some(("authorization", "Bearer fixture-token")),
            body_contains: None,
            response_content_type: "application/json",
            response_body: r#"{"id":44,"name":"Linux","status":"completed","conclusion":"success","html_url":"https://github.test/jobs/44","run_id":91}"#,
        },
        ExpectedRequest {
            method: "GET",
            path: "/repos/acme/widget/actions/jobs/44/logs",
            required_header: Some(("authorization", "Bearer fixture-token")),
            body_contains: None,
            response_content_type: "text/plain",
            response_body: "build complete\n",
        },
    ]);
    let provider = GithubApi::with_base_url(base_url, "fixture-token").unwrap();

    let output = provider
        .execute_command(
            &scope(),
            &ProviderCommand::ReadCiJobLog {
                job: CiJobHandle::new("44"),
            },
        )
        .unwrap();

    assert_eq!(
        output,
        ProviderCommandOutput::CiJobLog("build complete\n".to_owned())
    );
    server.join().unwrap().unwrap();
}
