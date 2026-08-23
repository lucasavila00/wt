use super::*;
use crate::api::test_server::{serve, ExpectedRequest};

const PULL_REQUEST: &str = r#"{"number":7,"node_id":"pull-request-7","html_url":"https://github.test/acme/widget/pull/7","title":"Fix login","body":"Fixes the login flow.","state":"closed","draft":false,"merged":false,"head":{"ref":"wt/fix-login","sha":"abc123","repo":{"full_name":"acme/widget"}},"base":{"ref":"main","sha":"def456","repo":{"full_name":"acme/widget"}},"mergeable":false}"#;
const ISSUE_COMMENT: &str = r#"{"id":123,"body":"General feedback.","html_url":"https://github.test/acme/widget/pull/7#issuecomment-123","issue_url":"https://api.github.test/repos/acme/widget/issues/7","user":{"login":"reviewer"},"created_at":"2026-08-22T10:00:00Z","updated_at":"2026-08-22T11:00:00Z"}"#;

#[test]
fn comment_mutations_reject_a_comment_from_another_pull_request_before_writing() {
    let wrong_mr = ISSUE_COMMENT.replace("/issues/7", "/issues/8").leak();
    for command in mutations() {
        let (base_url, server) = serve(vec![
            get("/repos/acme/widget/pulls/7", PULL_REQUEST),
            get("/repos/acme/widget/pulls/7", PULL_REQUEST),
            get("/repos/acme/widget/issues/comments/123", wrong_mr),
        ]);
        let error = GithubApi::with_base_url(base_url, "fixture-token")
            .unwrap()
            .execute_cli_command(&project_scope(), &command)
            .unwrap_err();

        assert_eq!(error.to_string(), "comment 123 does not belong to MR 7");
        server.join().unwrap().unwrap();
    }
}

#[test]
fn comment_mutations_require_gateway_attribution_before_writing() {
    for command in mutations() {
        let (base_url, server) = serve(vec![
            get("/repos/acme/widget/pulls/7", PULL_REQUEST),
            get("/repos/acme/widget/pulls/7", PULL_REQUEST),
            get("/repos/acme/widget/issues/comments/123", ISSUE_COMMENT),
        ]);
        let error = GithubApi::with_base_url(base_url, "fixture-token")
            .unwrap()
            .execute_cli_command(&project_scope(), &command)
            .unwrap_err();

        assert_eq!(
            error.to_string(),
            "comment is missing the WT world agent marker"
        );
        server.join().unwrap().unwrap();
    }
}

fn mutations() -> [WtToolsCommand; 2] {
    [
        WtToolsCommand::EditComment {
            mr: "7".into(),
            comment: "123".into(),
            body: "Updated.".to_owned(),
            confirm_merged: false,
        },
        WtToolsCommand::DeleteComment {
            mr: "7".into(),
            comment: "123".into(),
            confirm_merged: false,
        },
    ]
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
