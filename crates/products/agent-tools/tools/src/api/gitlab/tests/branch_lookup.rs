use super::*;
use crate::api::render_cli_command_output;

#[test]
fn shows_the_open_merge_request_for_an_explicit_branch() {
    let (base_url, server) = serve(vec![ExpectedRequest {
        method: "GET",
        path: "/api/v4/projects/acme%2Fwidget/merge_requests?state=opened&source_branch=wt%2Ffix-login&per_page=100",
        required_header: Some(("private-token", "fixture-token")),
        body_contains: None,
        response_content_type: "application/json",
        response_body: r#"[{"iid":8,"title":"Fix login","description":"Fixes the login flow.","web_url":"https://gitlab.test/acme/widget/-/merge_requests/8","state":"opened","draft":false,"sha":"abc123","source_branch":"wt/fix-login","target_branch":"main"}]"#,
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
    {"result":{"data":{"base":"main","body":"Fixes the login flow.","draft":false,"handle":"8","head":"abc123","jobs":[],"review_state":null,"state":"opened","threads":[],"title":"Fix login","url":"https://gitlab.test/acme/widget/-/merge_requests/8"},"type":"change_request"},"version":1}
    "###);
    server.join().unwrap().unwrap();
}

#[test]
fn rejects_a_branch_without_an_open_merge_request() {
    let (base_url, server) = serve(vec![ExpectedRequest {
        method: "GET",
        path: "/api/v4/projects/acme%2Fwidget/merge_requests?state=opened&source_branch=wt%2Fmissing&per_page=100",
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
        "no open merge request from branch `wt/missing`"
    );
    server.join().unwrap().unwrap();
}

#[test]
fn refuses_to_choose_between_bases() {
    let response = r#"[
        {"iid":8,"title":"Main","web_url":"https://gitlab.test/acme/widget/-/merge_requests/8","state":"opened","draft":false,"sha":"abc123","source_branch":"wt/fix-login","target_branch":"main"},
        {"iid":9,"title":"Release","web_url":"https://gitlab.test/acme/widget/-/merge_requests/9","state":"opened","draft":false,"sha":"abc123","source_branch":"wt/fix-login","target_branch":"release"}
    ]"#;
    let (base_url, server) = serve(vec![ExpectedRequest {
        method: "GET",
        path: "/api/v4/projects/acme%2Fwidget/merge_requests?state=opened&source_branch=wt%2Ffix-login&per_page=100",
        required_header: Some(("private-token", "fixture-token")),
        body_contains: None,
        response_content_type: "application/json",
        response_body: response,
    }]);
    let provider = GitlabApi::with_base_url(base_url, "fixture-token").unwrap();

    let error = provider
        .execute_cli_command(
            &project_scope(),
            &CliCommand::ShowMrForBranch {
                branch: "wt/fix-login".to_owned(),
            },
        )
        .unwrap_err();

    assert_eq!(
        error.to_string(),
        "GitLab returned 2 open merge requests from branch `wt/fix-login`; refusing to choose one"
    );
    server.join().unwrap().unwrap();
}
