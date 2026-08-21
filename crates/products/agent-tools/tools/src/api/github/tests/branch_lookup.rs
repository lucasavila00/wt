use super::*;
use crate::api::render_cli_command_output;

#[test]
fn shows_the_open_pull_request_for_an_explicit_branch() {
    let request = r#"{"number":7,"node_id":"pull-request-7","html_url":"https://github.test/acme/widget/pull/7","title":"Fix login","body":"Fixes the login flow.","state":"open","draft":false,"head":{"ref":"wt/fix-login","sha":"abc123","repo":{"full_name":"acme/widget"}},"base":{"ref":"main","sha":"def456","repo":{"full_name":"acme/widget"}}}"#;
    let (base_url, server) = serve(vec![
        ExpectedRequest {
            method: "GET",
            path: "/repos/acme/widget/pulls?state=open&head=acme%3Awt%2Ffix-login&per_page=100",
            required_header: Some(("authorization", "Bearer fixture-token")),
            body_contains: None,
            response_content_type: "application/json",
            response_body: leak_fixture(format!("[{request}]")),
        },
        ExpectedRequest {
            method: "GET",
            path: "/repos/acme/widget/pulls/7",
            required_header: Some(("authorization", "Bearer fixture-token")),
            body_contains: None,
            response_content_type: "application/json",
            response_body: request,
        },
        ExpectedRequest {
            method: "GET",
            path: "/repos/acme/widget/actions/runs?head_sha=abc123&per_page=100",
            required_header: Some(("authorization", "Bearer fixture-token")),
            body_contains: None,
            response_content_type: "application/json",
            response_body: r#"{"total_count":1,"workflow_runs":[{"id":91,"name":"CI","event":"pull_request","status":"in_progress","conclusion":null,"html_url":"https://github.test/runs/91","head_sha":"abc123","head_branch":"wt/fix-login","head_repository":{"full_name":"acme/widget"}}]}"#,
        },
        ExpectedRequest {
            method: "GET",
            path: "/repos/acme/widget/actions/runs/91/jobs?filter=latest&per_page=100",
            required_header: Some(("authorization", "Bearer fixture-token")),
            body_contains: None,
            response_content_type: "application/json",
            response_body: r#"{"total_count":1,"jobs":[{"id":44,"name":"Linux","status":"in_progress","conclusion":null,"html_url":"https://github.test/jobs/44","run_id":91}]}"#,
        },
    ]);
    let provider = GithubApi::with_base_url(base_url, "fixture-token").unwrap();

    let output = provider
        .execute_cli_command(
            &project_scope(),
            &WtToolsCommand::ShowMrForBranch {
                branch: "wt/fix-login".to_owned(),
            },
        )
        .unwrap();

    insta::assert_snapshot!(render_cli_command_output(output), @r###"
    {"type":"change_request","data":{"handle":"7","url":"https://github.test/acme/widget/pull/7","title":"Fix login","body":"Fixes the login flow.","state":"open","draft":false,"head":"abc123","base":"main","conflict_state":"pending","review_state":null,"threads":[],"jobs":[{"handle":"44","run":"91","name":"Linux","state":"in_progress","url":"https://github.test/jobs/44"}]}}
    "###);
    server.join().unwrap().unwrap();
}

#[test]
fn rejects_a_branch_without_an_open_pull_request() {
    let (base_url, server) = serve(vec![ExpectedRequest {
        method: "GET",
        path: "/repos/acme/widget/pulls?state=open&head=acme%3Awt%2Fmissing&per_page=100",
        required_header: Some(("authorization", "Bearer fixture-token")),
        body_contains: None,
        response_content_type: "application/json",
        response_body: "[]",
    }]);
    let provider = GithubApi::with_base_url(base_url, "fixture-token").unwrap();

    let error = provider
        .execute_cli_command(
            &project_scope(),
            &WtToolsCommand::ShowMrForBranch {
                branch: "wt/missing".to_owned(),
            },
        )
        .unwrap_err();

    assert_eq!(
        error.to_string(),
        "no open pull request from branch `wt/missing`"
    );
    server.join().unwrap().unwrap();
}

#[test]
fn refuses_to_choose_between_bases() {
    let response = r#"[
        {"number":7,"node_id":"pull-request-7","html_url":"https://github.test/acme/widget/pull/7","title":"Main","state":"open","draft":false,"head":{"ref":"wt/fix-login","sha":"abc123","repo":{"full_name":"acme/widget"}},"base":{"ref":"main","sha":"def456","repo":{"full_name":"acme/widget"}}},
        {"number":8,"node_id":"pull-request-8","html_url":"https://github.test/acme/widget/pull/8","title":"Release","state":"open","draft":false,"head":{"ref":"wt/fix-login","sha":"abc123","repo":{"full_name":"acme/widget"}},"base":{"ref":"release","sha":"def456","repo":{"full_name":"acme/widget"}}}
    ]"#;
    let (base_url, server) = serve(vec![ExpectedRequest {
        method: "GET",
        path: "/repos/acme/widget/pulls?state=open&head=acme%3Awt%2Ffix-login&per_page=100",
        required_header: Some(("authorization", "Bearer fixture-token")),
        body_contains: None,
        response_content_type: "application/json",
        response_body: response,
    }]);
    let provider = GithubApi::with_base_url(base_url, "fixture-token").unwrap();

    let error = provider
        .execute_cli_command(
            &project_scope(),
            &WtToolsCommand::ShowMrForBranch {
                branch: "wt/fix-login".to_owned(),
            },
        )
        .unwrap_err();

    assert_eq!(
        error.to_string(),
        "GitHub returned 2 open pull requests from branch `wt/fix-login`; refusing to choose one"
    );
    server.join().unwrap().unwrap();
}
