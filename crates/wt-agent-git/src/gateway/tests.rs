use super::*;
use diesel::prelude::*;
use wt_git_core::{validate_push, PushViolation};
use wt_registry::schema::{disks, guests, worlds};

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
    let policy = WritePolicy::new("refs/heads/wt/", []).unwrap();
    assert!(validate_push(&command("refs/heads/wt/fix"), &policy).is_ok());
    assert!(validate_push(&command("refs/heads/fix"), &policy).is_err());
    assert!(validate_push(&command("refs/tags/v1"), &policy).is_err());
}

#[test]
fn help_is_the_complete_command_contract() {
    insta::assert_snapshot!(HELP);
}

#[test]
fn git_header_explains_the_environment_without_prior_context() {
    insta::assert_snapshot!(git_context_header("git@github.com:group/project.git"));
}

#[test]
fn cli_status_and_unavailable_command_are_actionable() {
    insta::assert_snapshot!("cli_unavailable", cli_unavailable());
}

#[test]
fn world_prompt_does_not_require_a_checkout_or_provider_api() {
    let temp = tempfile::tempdir().unwrap();
    let gateway = Gateway::open(GatewayConfig {
        state_file: temp.path().join("gateway.json"),
        database_path: temp.path().join("instances.db"),
        providers: vec![Provider::Local {
            host: "github.com".into(),
            repositories: temp.path().to_owned(),
            api: None,
        }],
    })
    .unwrap();

    let output = gateway
        .serve_cli(&["world-prompt".into()], None, None, None, &test_grant())
        .unwrap();

    insta::assert_snapshot!(output);
}

#[test]
fn opening_the_gateway_revokes_legacy_repository_scoped_grants() {
    let temp = tempfile::tempdir().unwrap();
    let state_file = temp.path().join("gateway.json");
    fs::write(
        &state_file,
        r#"{"grants":[{"id":"legacy","token":"secret","world_id":"world","source":"git@github.com:acme/widget.git","base":"main","prefix":"wt/","revoked":false}]}"#,
    )
    .unwrap();

    let gateway = Gateway::open(GatewayConfig {
        state_file: state_file.clone(),
        database_path: temp.path().join("instances.db"),
        providers: vec![Provider::Local {
            host: "github.com".into(),
            repositories: temp.path().to_owned(),
            api: None,
        }],
    })
    .unwrap();

    assert!(gateway.state.lock().unwrap().grants[0].revoked);
    let persisted: State = serde_json::from_slice(&fs::read(state_file).unwrap()).unwrap();
    assert!(persisted.grants[0].revoked);
}

#[test]
fn agent_git_reports_are_stored_for_the_authenticated_world_without_a_provider_api() {
    let temp = tempfile::tempdir().unwrap();
    let database_path = temp.path().join("instances.db");
    let registry = wt_registry::Registry::open(&database_path).unwrap();
    let world_id = insert_world(&registry);
    let gateway = Gateway::open(GatewayConfig {
        state_file: temp.path().join("gateway.json"),
        database_path,
        providers: vec![Provider::Local {
            host: "github.com".into(),
            repositories: temp.path().to_owned(),
            api: None,
        }],
    })
    .unwrap();
    let mut grant = test_grant();
    grant.world_id = world_id.to_string();

    let commands = [
        r#"{"action":"report_ag_git_bug","description":"job logs disappear"}"#,
        r#"{"action":"report_ag_git_issue","description":"the hint is unclear"}"#,
        r#"{"action":"suggest_ag_git_improvement","description":"show the check name"}"#,
        r#"{"action":"request_ag_git_feature","description":"support CI search"}"#,
    ];
    let outputs = commands
        .map(|command| {
            gateway
                .serve_cli(&[command.into()], None, None, None, &grant)
                .unwrap()
        })
        .concat();

    insta::assert_snapshot!(outputs, @r###"
    Recorded ag-git report for this world.
    Recorded ag-git report for this world.
    Recorded ag-git report for this world.
    Recorded ag-git report for this world.
    "###);
    let reports = registry.list_agent_git_reports("alice").unwrap();
    assert_eq!(reports.len(), 4);
    assert!(reports.iter().all(|report| report.world_id == world_id));
    assert_eq!(
        reports.iter().map(|report| report.kind).collect::<Vec<_>>(),
        vec![
            wt_registry::AgentGitReportKind::Bug,
            wt_registry::AgentGitReportKind::Issue,
            wt_registry::AgentGitReportKind::Improvement,
            wt_registry::AgentGitReportKind::FeatureRequest,
        ]
    );
    assert_eq!(reports[0].description, "job logs disappear");
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
    insta::assert_snapshot!(
        service::push_result_message(
            true,
            &command(&"a".repeat(40), "refs/heads/wt/fix-login"),
            &response("ok refs/heads/wt/fix-login"),
            true,
        )
        .unwrap(),
        @r###"
    Published branch `wt/fix-login`.
    Inspect its open MR with:
      ag-git '{"action":"show_mr_for_branch","branch":"wt/fix-login"}'
    If that reports no open MR, run `ag-git --help` and open one with an explicit base.
    Inspect CI with:
      ag-git '{"action":"list_ci","commit":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}'
    "###
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
        service::push_rejection_message(&PushViolation::Unauthorized {
            reference: "refs/heads/fix-login".to_owned()
        })
    );
}

fn test_grant() -> GrantRecord {
    GrantRecord {
        id: "id".to_owned(),
        token: "token".to_owned(),
        world_id: "world".to_owned(),
        legacy_source: None,
        legacy_base: None,
        legacy_prefix: None,
        revoked: false,
    }
}

fn insert_world(registry: &wt_registry::Registry) -> Uuid {
    let id = Uuid::new_v4();
    let disk_id = Uuid::new_v4();
    registry
        .transaction::<_, wt_registry::RegistryError>(|connection| {
            diesel::insert_into(disks::table)
                .values(disks::id.eq(disk_id.to_string()))
                .execute(connection)?;
            diesel::insert_into(guests::table)
                .values((
                    guests::id.eq(id.to_string()),
                    guests::kind.eq("devcontainer"),
                    guests::backend_id.eq(format!("wt-{}", id.simple())),
                    guests::disk_id.eq(disk_id.to_string()),
                    guests::vcpus.eq(1_i64),
                    guests::memory_mib.eq(1024_i64),
                    guests::disk_gib.eq(10_i64),
                    guests::compute_reserved.eq(true),
                ))
                .execute(connection)?;
            diesel::insert_into(worlds::table)
                .values((
                    worlds::id.eq(id.to_string()),
                    worlds::owner.eq("alice"),
                    worlds::name.eq("checkout"),
                    worlds::status.eq("running"),
                    worlds::setup_fingerprint.eq("fingerprint"),
                    worlds::ssh_host_keys.eq("[]"),
                ))
                .execute(connection)?;
            Ok(())
        })
        .unwrap();
    id
}
