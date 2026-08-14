use super::*;

#[test]
fn parses_supported_sources_without_shell_syntax() {
    let source = parse_source("git@example.test:group/repo.git").unwrap();
    assert_eq!(source.host, "example.test");
    assert_eq!(source.path, "group/repo.git");
    assert!(parse_source("git@example.test:group/repo;touch-pwned").is_err());
    assert!(parse_source("git@example.test:../repo.git").is_err());
}

#[test]
fn push_scope_allows_only_prefixed_heads() {
    let command = |reference: &str| {
        let payload = format!(
            "{} {} {}\0report-status\n",
            "0".repeat(40),
            "a".repeat(40),
            reference
        );
        format!("{:04x}{payload}0000", payload.len() + 4).into_bytes()
    };
    assert!(validate_push(&command("refs/heads/wt/fix"), "wt/").is_ok());
    assert!(validate_push(&command("refs/heads/fix"), "wt/").is_err());
    assert!(validate_push(&command("refs/tags/v1"), "wt/").is_err());
}

#[test]
fn help_is_the_complete_command_contract() {
    insta::assert_snapshot!(HELP);
}

#[test]
fn git_header_explains_the_environment_without_prior_context() {
    let grant = test_grant();
    insta::assert_snapshot!(git_context_header(&grant));
}

#[test]
fn cli_status_and_unavailable_command_are_actionable() {
    insta::assert_snapshot!("cli_unavailable", cli_unavailable());
}

#[test]
fn push_messages_cover_publish_delete_and_rejection() {
    let command = |new: &str, reference: &str| {
        let payload = format!("{} {new} {reference}\0report-status\n", "0".repeat(40));
        format!("{:04x}{payload}0000", payload.len() + 4).into_bytes()
    };
    let response = |status: &str| {
        let mut report = Vec::new();
        write_packet(&mut report, b"unpack ok\n").unwrap();
        write_packet(&mut report, format!("{status}\n").as_bytes()).unwrap();
        report.extend_from_slice(b"0000");
        let mut packet = vec![1];
        packet.extend_from_slice(&report);
        let mut response = Vec::new();
        write_packet(&mut response, &packet).unwrap();
        response.extend_from_slice(b"0000");
        response
    };
    assert_eq!(
        successful_push_updates(
            &command(&"a".repeat(40), "refs/heads/wt/fix-login"),
            &response("ok refs/heads/wt/fix-login"),
            true,
        )
        .unwrap(),
        vec![("a".repeat(40), "wt/fix-login".to_owned())]
    );
    assert_eq!(
        successful_push_updates(
            &command(&"0".repeat(40), "refs/heads/wt/fix-login"),
            &response("ok refs/heads/wt/fix-login"),
            true,
        )
        .unwrap(),
        vec![("0".repeat(40), "wt/fix-login".to_owned())]
    );
    assert!(successful_push_updates(
        &command(&"a".repeat(40), "refs/heads/wt/fix-login"),
        &response("ng refs/heads/wt/fix-login protected branch"),
        true,
    )
    .unwrap()
    .is_empty());
    insta::assert_snapshot!(
        "push_rejected",
        validate_push(&command(&"a".repeat(40), "refs/heads/fix-login"), "wt/")
            .unwrap_err()
            .to_string()
    );
}

fn test_grant() -> GrantRecord {
    GrantRecord {
        id: "id".to_owned(),
        token: "token".to_owned(),
        world_id: "world".to_owned(),
        source: "git@github.com:group/project.git".to_owned(),
        base: "main".to_owned(),
        prefix: BRANCH_PREFIX.to_owned(),
        revoked: false,
    }
}
