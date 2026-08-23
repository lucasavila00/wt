use super::*;

#[test]
fn current_context_mutations_refuse_merged_merge_requests() {
    let response = Box::leak(
        MERGE_REQUEST_RESPONSE
            .replace("\"state\": \"opened\"", "\"state\": \"merged\"")
            .into_boxed_str(),
    );
    let commands = [
        ProviderCommand::MarkChangeRequestReady,
        ProviderCommand::MarkChangeRequestDraft,
        ProviderCommand::AddChangeRequestComment {
            body: "Done".to_owned(),
        },
        ProviderCommand::EditChangeRequest {
            title: Some("Better title".to_owned()),
            body: None,
        },
        ProviderCommand::ReplyToReviewThread {
            thread: ReviewThreadHandle::new("discussion-1"),
            body: "Done".to_owned(),
        },
        ProviderCommand::SetReviewThreadResolved {
            thread: ReviewThreadHandle::new("discussion-1"),
            resolved: true,
        },
        ProviderCommand::CloseChangeRequest,
        ProviderCommand::ReopenChangeRequest,
    ];

    for command in commands {
        let (base_url, server) = serve(vec![ExpectedRequest {
            method: "POST",
            path: "/api/graphql",
            required_header: Some(("private-token", "fixture-token")),
            body_contains: Some("GitlabReadMergeRequest"),
            response_content_type: "application/json",
            response_body: response,
        }]);
        let provider = GitlabApi::with_base_url(base_url, "fixture-token").unwrap();

        let error = provider.execute_command(&scope(), &command).unwrap_err();

        assert_eq!(
            error.to_string(),
            "MR !8 is already merged; wt-tools refuses to modify it"
        );
        server.join().unwrap().unwrap();
    }
}
