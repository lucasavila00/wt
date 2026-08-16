use super::*;
use crate::api::render_cli_command_output;

#[test]
fn shows_the_open_merge_request_for_an_explicit_branch() {
    let (base_url, server) = serve(vec![ExpectedRequest {
        method: "GET",
        path: "/api/v4/projects/acme%2Fwidget/merge_requests?state=opened&source_branch=wt%2Ffix-login&target_branch=main&per_page=100",
        required_header: Some(("private-token", "fixture-token")),
        body_contains: None,
        response_content_type: "application/json",
        response_body: r#"[{"iid":8,"title":"Fix login","web_url":"https://gitlab.test/acme/widget/-/merge_requests/8","state":"opened","draft":false,"sha":"abc123","source_branch":"wt/fix-login","target_branch":"main"}]"#,
    }]);
    let provider = GitlabApi::with_base_url(base_url, "fixture-token").unwrap();

    let output = provider
        .execute_cli_command(
            &project_scope(),
            &CliCommand::ShowMrForBranch {
                branch: "wt/fix-login".to_owned(),
            },
        )
        .unwrap();

    insta::assert_snapshot!(render_cli_command_output(output), @r###"
    MR: 8
    State: opened
    Title: Fix login
    Head: abc123
    Base: main
    URL: https://gitlab.test/acme/widget/-/merge_requests/8
    "###);
    server.join().unwrap().unwrap();
}

#[test]
fn rejects_a_branch_without_an_open_merge_request() {
    let (base_url, server) = serve(vec![ExpectedRequest {
        method: "GET",
        path: "/api/v4/projects/acme%2Fwidget/merge_requests?state=opened&source_branch=wt%2Fmissing&target_branch=main&per_page=100",
        required_header: Some(("private-token", "fixture-token")),
        body_contains: None,
        response_content_type: "application/json",
        response_body: "[]",
    }]);
    let provider = GitlabApi::with_base_url(base_url, "fixture-token").unwrap();

    let error = provider
        .execute_cli_command(
            &project_scope(),
            &CliCommand::ShowMrForBranch {
                branch: "wt/missing".to_owned(),
            },
        )
        .unwrap_err();

    assert_eq!(
        error.to_string(),
        "no open merge request from branch `wt/missing` to `main`"
    );
    server.join().unwrap().unwrap();
}
