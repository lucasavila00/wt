use super::*;
use crate::{CodexSessionStartSource, CodexSessionStartSourceKind};
use diesel::prelude::*;
use wt_git_smart_protocol::{validate_push, PushViolation};
use wt_workload_registry::schema::worlds;

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
    insta::assert_snapshot!(wt_tools_help());
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
fn worst_case_ci_log_tail_fits_the_transport_header() {
    let output = api::render_cli_command_output(api::ProviderCommandOutput::CiJobLog {
        log: "\0".repeat(1024 * 1024),
        truncated: true,
    });
    let mut encoded = Vec::new();

    crate::write_json_line(&mut encoded, &TransportResponse::with_message(output)).unwrap();

    assert!(encoded.len() < 8 * 1024 * 1024);
    let decoded: TransportResponse = crate::read_json_line(&mut encoded.as_slice()).unwrap();
    assert!(decoded.ok);
}

#[test]
fn provider_targets_are_validated_and_unambiguous() {
    assert!(validate_repository("acme/widget").is_ok());
    for invalid in ["", "/acme/widget", "acme/widget.git", "acme/../widget"] {
        assert!(validate_repository(invalid).is_err());
    }

    let temp = tempfile::tempdir().unwrap();
    let provider = |host: &str| Provider::Local {
        host: host.into(),
        repositories: temp.path().to_owned(),
        api: Some(FixtureApi {
            kind: ProviderKind::GitHub,
            base_url: "http://github.test".into(),
            token_file: temp.path().join("token"),
        }),
    };
    let error = Gateway::open(GatewayConfig {
        state_file: temp.path().join("gateway.json"),
        database_path: temp.path().join("instances.db"),
        providers: vec![provider("github.test"), provider("enterprise.test")],
    })
    .err()
    .unwrap();
    assert_eq!(error.to_string(), "duplicate GitHub API provider");
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
        .serve_cli(&["world-prompt".into()], &test_grant())
        .unwrap();

    insta::assert_snapshot!(output);
}

#[test]
fn gateway_state_rejects_unknown_grant_fields() {
    let state = serde_json::from_str::<State>(
        r#"{"grants":[{"id":"id","token":"token","world_id":"world","revoked":false,"unexpected":true}]}"#,
    );
    assert!(state.is_err());
}

#[test]
fn agent_tool_reports_are_stored_for_the_authenticated_world_without_a_provider_api() {
    let temp = tempfile::tempdir().unwrap();
    let database_path = temp.path().join("instances.db");
    let registry = wt_workload_registry::Registry::open(&database_path).unwrap();
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

    for (name, command) in [
        (
            "report_wt_tool_bug",
            r#"{"action":"report_wt_tool_bug","description":"job logs disappear"}"#,
        ),
        (
            "report_wt_tool_issue",
            r#"{"action":"report_wt_tool_issue","description":"the hint is unclear"}"#,
        ),
        (
            "suggest_wt_tool_improvement",
            r#"{"action":"suggest_wt_tool_improvement","description":"show the check name"}"#,
        ),
        (
            "request_wt_tool_feature",
            r#"{"action":"request_wt_tool_feature","description":"support CI search"}"#,
        ),
    ] {
        let output = gateway
            .serve_cli(&[format!(r#"{{"command":{command}}}"#)], &grant)
            .unwrap();
        insta::assert_snapshot!(name, output);
    }
    let reports = registry.list_agent_tool_reports("alice").unwrap();
    assert_eq!(reports.len(), 4);
    assert!(reports.iter().all(|report| report.world_id == world_id));
    assert_eq!(
        reports.iter().map(|report| report.kind).collect::<Vec<_>>(),
        vec![
            wt_workload_registry::AgentToolReportKind::Bug,
            wt_workload_registry::AgentToolReportKind::Issue,
            wt_workload_registry::AgentToolReportKind::Improvement,
            wt_workload_registry::AgentToolReportKind::FeatureRequest,
        ]
    );
    assert_eq!(reports[0].description, "job logs disappear");
}

#[test]
fn codex_events_upsert_latest_state_for_the_authenticated_world() {
    let temp = tempfile::tempdir().unwrap();
    let database_path = temp.path().join("instances.db");
    let registry = wt_workload_registry::Registry::open(&database_path).unwrap();
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
    let session_id = Uuid::new_v4();
    let mut event = CodexSessionEvent {
        session_id,
        cwd: "/home/wt/project".into(),
        repository_root: Some("/home/wt/project".into()),
        repository_url: Some("git@github.com:acme/project.git".into()),
        git_branch: Some("wt/cards".into()),
        tmux_session: "wt-host".into(),
        pane_id: "%3".into(),
        kind: CodexSessionEventKind::UserPromptSubmit,
        session_start_source: None,
    };

    gateway.store_codex_session_event(&event, &grant).unwrap();
    event.kind = CodexSessionEventKind::SessionStart;
    event.session_start_source = Some(CodexSessionStartSource {
        kind: CodexSessionStartSourceKind::Compact,
        raw: "compact".into(),
    });
    gateway.store_codex_session_event(&event, &grant).unwrap();

    let reports = registry.list_codex_session_reports("alice").unwrap();
    assert_eq!(reports.len(), 1);
    assert_eq!(
        reports[0].state,
        wt_workload_registry::CodexSessionState::Unknown
    );
    assert_eq!(reports[0].session_start_source.as_deref(), Some("compact"));

    event.kind = CodexSessionEventKind::Stop;
    event.session_start_source = None;
    gateway.store_codex_session_event(&event, &grant).unwrap();

    let reports = registry.list_codex_session_reports("alice").unwrap();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].world_id, world_id);
    assert_eq!(reports[0].session_id, session_id);
    assert_eq!(
        reports[0].state,
        wt_workload_registry::CodexSessionState::NeedsAttention
    );
    assert_eq!(reports[0].tmux_session, "wt-host");
    assert_eq!(reports[0].pane_id, "%3");
    assert_eq!(reports[0].session_start_source, None);
}

#[test]
fn new_codex_session_in_a_pane_deactivates_the_previous_session() {
    let temp = tempfile::tempdir().unwrap();
    let database_path = temp.path().join("instances.db");
    let registry = wt_workload_registry::Registry::open(&database_path).unwrap();
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
    let previous_session_id = Uuid::new_v4();
    let mut event = CodexSessionEvent {
        session_id: previous_session_id,
        cwd: "/home/wt/project".into(),
        repository_root: None,
        repository_url: None,
        git_branch: None,
        tmux_session: "wt-host".into(),
        pane_id: "%3".into(),
        kind: CodexSessionEventKind::UserPromptSubmit,
        session_start_source: None,
    };
    gateway.store_codex_session_event(&event, &grant).unwrap();

    let replacement_session_id = Uuid::new_v4();
    event.session_id = replacement_session_id;
    event.kind = CodexSessionEventKind::SessionStart;
    event.session_start_source = Some(CodexSessionStartSource {
        kind: CodexSessionStartSourceKind::Clear,
        raw: "clear".into(),
    });
    gateway.store_codex_session_event(&event, &grant).unwrap();

    let reports = registry.list_codex_session_reports("alice").unwrap();
    assert_eq!(reports.len(), 2);
    assert_eq!(
        reports
            .iter()
            .find(|report| report.session_id == previous_session_id)
            .unwrap()
            .state,
        wt_workload_registry::CodexSessionState::Inactive
    );
    assert_eq!(
        reports
            .iter()
            .find(|report| report.session_id == replacement_session_id)
            .unwrap()
            .state,
        wt_workload_registry::CodexSessionState::Unknown
    );

    event.session_id = previous_session_id;
    event.kind = CodexSessionEventKind::SessionEnd;
    event.session_start_source = None;
    gateway.store_codex_session_event(&event, &grant).unwrap();

    let reports = registry.list_codex_session_reports("alice").unwrap();
    assert_eq!(
        reports
            .iter()
            .find(|report| report.session_id == replacement_session_id)
            .unwrap()
            .state,
        wt_workload_registry::CodexSessionState::Unknown
    );
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
    let updates = successful_push_updates(
        &command(&"a".repeat(40), "refs/heads/wt/fix-login"),
        &response("ok refs/heads/wt/fix-login"),
        true,
    )
    .unwrap();
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].previous_oid, "0".repeat(40));
    assert_eq!(updates[0].new_oid, "a".repeat(40));
    assert_eq!(updates[0].reference, "refs/heads/wt/fix-login");
    insta::assert_snapshot!(
        service::push_result_message(
            Some((ProviderKind::GitHub, "acme/widget")),
            &command(&"a".repeat(40), "refs/heads/wt/fix-login"),
            &response("ok refs/heads/wt/fix-login"),
            true,
        )
        .unwrap(),
        @r###"
    Published branch `wt/fix-login`.
    Inspect its open MR with:
      wt-tools '{"command":{"action":"show_mr_for_branch","branch":"wt/fix-login"},"target":{"provider":"github","repository":"acme/widget"}}'
    If that reports no open MR, run `wt-tools --help` and open one with an explicit base.
    Inspect CI with:
      wt-tools '{"command":{"action":"list_ci","commit":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},"target":{"provider":"github","repository":"acme/widget"}}'
    "###
    );
    let updates = successful_push_updates(
        &command(&"0".repeat(40), "refs/heads/wt/fix-login"),
        &response("ok refs/heads/wt/fix-login"),
        true,
    )
    .unwrap();
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].new_oid, "0".repeat(40));
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
        revoked: false,
    }
}

fn insert_world(registry: &wt_workload_registry::Registry) -> Uuid {
    let id = Uuid::new_v4();
    registry
        .transaction::<_, wt_workload_registry::RegistryError>(|connection| {
            diesel::insert_into(worlds::table)
                .values((
                    worlds::id.eq(id.to_string()),
                    worlds::backend_id.eq(format!("wt-{}", id.simple())),
                    worlds::disk_id.eq(Uuid::new_v4().to_string()),
                    worlds::vcpus.eq(1_i64),
                    worlds::memory_mib.eq(1024_i64),
                    worlds::disk_gib.eq(10_i64),
                    worlds::disk_reserved_gib.eq(10_i64),
                    worlds::compute_reserved.eq(true),
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
