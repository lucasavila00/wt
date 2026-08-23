use super::*;

#[test]
fn current_context_mutations_require_confirmation_for_merged_merge_requests() {
    let response = Box::leak(
        MERGE_REQUEST_RESPONSE
            .replace("\"state\": \"opened\"", "\"state\": \"merged\"")
            .into_boxed_str(),
    );
    let commands = [
        ProviderCommand::MarkChangeRequestReady {
            confirm_merged: false,
        },
        ProviderCommand::MarkChangeRequestDraft {
            confirm_merged: false,
        },
        ProviderCommand::AddChangeRequestComment {
            body: "Done".to_owned(),
            confirm_merged: false,
        },
        ProviderCommand::EditChangeRequest {
            title: Some("Better title".to_owned()),
            body: None,
            confirm_merged: false,
        },
        ProviderCommand::ReplyToReviewThread {
            thread: ReviewThreadHandle::new("discussion-1"),
            body: "Done".to_owned(),
            confirm_merged: false,
        },
        ProviderCommand::SetReviewThreadResolved {
            thread: ReviewThreadHandle::new("discussion-1"),
            resolved: true,
            confirm_merged: false,
        },
        ProviderCommand::CloseChangeRequest {
            confirm_merged: false,
        },
        ProviderCommand::ReopenChangeRequest {
            confirm_merged: false,
        },
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
            "MR !8 is already merged; rerun with `confirm_merged`: true to modify it"
        );
        server.join().unwrap().unwrap();
    }
}
