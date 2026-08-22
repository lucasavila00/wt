use super::*;

#[test]
fn opens_merge_request_as_draft_through_typed_graphql_mutations() {
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
        ExpectedRequest {
            method: "POST",
            path: "/api/graphql",
            required_header: Some(("private-token", "fixture-token")),
            body_contains: Some("GitlabSetMergeRequestDraft"),
            response_content_type: "application/json",
            response_body: r#"{"data":{"mergeRequestSetDraft":{"errors":[],"mergeRequest":{"id":"merge-request-8","iid":"8","webUrl":"https://gitlab.test/acme/widget/-/merge_requests/8"}}}}"#,
        },
        ExpectedRequest {
            method: "POST",
            path: "/api/graphql",
            required_header: Some(("private-token", "fixture-token")),
            body_contains: Some("GitlabReadMergeRequest"),
            response_content_type: "application/json",
            response_body: MERGE_REQUEST_RESPONSE
                .replace("\"draft\": false", "\"draft\": true")
                .leak(),
        },
    ]);
    let provider = GitlabApi::with_base_url(base_url, "fixture-token").unwrap();

    let output = provider
        .execute_command(&scope(), &ProviderCommand::OpenChangeRequest)
        .unwrap();

    let ProviderCommandOutput::ChangeRequest(request) = output else {
        panic!("expected opened merge request");
    };
    assert_eq!(request.handle, "!8");
    assert!(request.draft);
    server.join().unwrap().unwrap();
}
